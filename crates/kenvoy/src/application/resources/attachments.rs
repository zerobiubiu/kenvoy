use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::domain::{
    authorization,
    model::{
        ApplicationIR, FieldDefinition, FieldKind, MAX_UPLOAD_SIZE, Operation, Role, UploadedFile,
    },
};

use super::{ResourceError, ResourceRepository, ResourceService};

pub struct NewUpload {
    pub resource: String,
    pub field: String,
    pub owner: String,
    pub name: String,
    pub content_type: String,
    pub data: Vec<u8>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    pub file: UploadedFile,
    pub resource: String,
    pub field: String,
    pub record_id: Option<i64>,
    pub owner: String,
    pub expires_at: i64,
}

pub fn upload_owner(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut hash = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        hash.push(HEX[(byte >> 4) as usize] as char);
        hash.push(HEX[(byte & 15) as usize] as char);
    }
    hash
}

fn now<E: std::error::Error + 'static>() -> Result<i64, ResourceError<E>> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| i64::try_from(time.as_secs()).ok())
        .ok_or(ResourceError::InvalidTime)
}

pub struct UploadField<'a> {
    application: &'a ApplicationIR,
    resource: &'a str,
    field: &'a FieldDefinition,
}

impl UploadField<'_> {
    pub fn authorize(
        &self,
        operation: Operation,
        role: Role,
        form: Option<&str>,
    ) -> Result<(), authorization::AuthorizationError> {
        let fields = Map::from_iter([(self.field.name.clone(), Value::Null)]);
        authorization::policy(
            self.application,
            self.resource,
            operation,
            role,
            form,
            Some(&fields),
        )?;
        Ok(())
    }

    pub fn max_size(&self) -> Option<usize> {
        self.field
            .upload
            .as_ref()
            .and_then(|constraints| usize::try_from(constraints.max_size.min(MAX_UPLOAD_SIZE)).ok())
    }
}

impl<R: ResourceRepository> ResourceService<R> {
    pub fn upload_field<'a>(
        &self,
        application: &'a ApplicationIR,
        resource: &'a str,
        field: &str,
    ) -> Result<UploadField<'a>, ResourceError<R::Error>> {
        let field = application
            .resource(resource)
            .and_then(|resource| resource.field(field))
            .filter(|field| field.kind == FieldKind::File && field.upload.is_some())
            .ok_or(ResourceError::Missing)?;
        Ok(UploadField {
            application,
            resource,
            field,
        })
    }

    pub fn accept_upload(
        &self,
        expires_at: i64,
        accepted_generation: u64,
        current_generation: u64,
    ) -> Result<(), ResourceError<R::Error>> {
        if accepted_generation != current_generation {
            return Err(ResourceError::UploadGenerationChanged);
        }
        if expires_at <= now()? {
            return Err(ResourceError::UploadSessionExpired);
        }
        Ok(())
    }

    pub async fn stage_upload(
        &self,
        upload: NewUpload,
    ) -> Result<UploadedFile, ResourceError<R::Error>> {
        self.repository
            .stage_upload(upload)
            .await
            .map_err(ResourceError::Repository)
    }

    pub async fn file(
        &self,
        application: &ApplicationIR,
        id: &str,
        role: Role,
        has_session: bool,
        token: Option<&str>,
    ) -> Result<StoredFile, ResourceError<R::Error>> {
        let stored = self
            .repository
            .file_info(id)
            .await
            .map_err(ResourceError::Repository)?
            .ok_or(ResourceError::Missing)?;
        application
            .resource(&stored.resource)
            .and_then(|resource| resource.field(&stored.field))
            .filter(|field| field.kind == FieldKind::File)
            .ok_or(ResourceError::Missing)?;
        if stored.record_id.is_some() {
            let fields = Map::from_iter([(stored.field.clone(), Value::Null)]);
            authorization::policy(
                application,
                &stored.resource,
                Operation::Read,
                role,
                None,
                Some(&fields),
            )?;
        } else if !has_session
            || stored.expires_at <= now()?
            || token.is_none_or(|token| upload_owner(token) != stored.owner)
        {
            return Err(ResourceError::Missing);
        }
        Ok(stored)
    }

    pub async fn file_content(&self, id: &str) -> Result<Vec<u8>, ResourceError<R::Error>> {
        self.repository
            .file_content(id)
            .await
            .map_err(ResourceError::Repository)?
            .ok_or(ResourceError::Missing)
    }

    pub async fn delete_upload(
        &self,
        id: &str,
        owner: &str,
    ) -> Result<(), ResourceError<R::Error>> {
        if !self
            .repository
            .delete_upload(id, owner)
            .await
            .map_err(ResourceError::Repository)?
        {
            return Err(ResourceError::Missing);
        }
        Ok(())
    }
}
