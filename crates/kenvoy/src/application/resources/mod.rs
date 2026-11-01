//! 资源用例协调领域授权与存储事务；事务内的数据校验由仓储完成。

mod attachments;
mod navigation;
mod ports;
mod query;

pub use attachments::{NewUpload, StoredFile, UploadField, upload_owner};
pub use navigation::{PageAccess, page};
pub use ports::{RepositoryFailure, ResourceRepository};
pub use query::{
    FieldFilter, FilterOperator, ListQuery, ListResult, OptionItem, OptionsQuery, OptionsResult,
    Record,
};

use serde_json::{Map, Value};

use crate::domain::{
    authorization::{self, AuthorizationError},
    model::{ApplicationIR, FieldDefinition, Operation, PermissionDefinition, Role},
};

#[derive(Debug, thiserror::Error)]
pub enum ResourceError<E: std::error::Error + 'static> {
    #[error(transparent)]
    Authorization(#[from] AuthorizationError),
    #[error(transparent)]
    Repository(E),
    #[error("请求的资源不存在")]
    Missing,
    #[error("上传期间应用已更新，请重新选择文件")]
    UploadGenerationChanged,
    #[error("会话已过期，请重新选择文件")]
    UploadSessionExpired,
    #[error("系统时间无效")]
    InvalidTime,
}

#[derive(Clone)]
pub struct ResourceService<R> {
    repository: R,
}

impl<R: ResourceRepository> ResourceService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn authorize<'a>(
        &'a self,
        application: &'a ApplicationIR,
        resource: &'a str,
        operation: Operation,
        role: Role,
        form: Option<&str>,
        fields: Option<&Map<String, Value>>,
    ) -> Result<AuthorizedResource<'a, R>, ResourceError<R::Error>> {
        let permission =
            authorization::policy(application, resource, operation, role, form, fields)?;
        Ok(AuthorizedResource {
            service: self,
            application,
            resource,
            role,
            permission,
        })
    }

    pub fn options_field<'a>(
        &'a self,
        application: &'a ApplicationIR,
        resource: &'a str,
        field_name: &'a str,
        role: Role,
    ) -> Result<OptionsField<'a, R>, ResourceError<R::Error>> {
        let field = application
            .resource(resource)
            .and_then(|resource| resource.field(field_name))
            .ok_or(ResourceError::Missing)?;
        let source = field
            .options_source
            .as_ref()
            .ok_or(ResourceError::Missing)?;
        if !authorization::permits(role, source.permission) {
            return Err(AuthorizationError::Denied(role).into());
        }
        Ok(OptionsField {
            service: self,
            application,
            resource,
            field_name,
            field,
            role,
        })
    }
}

/// 授权结果在同代模型读屏障内消费，写入仍由仓储在原事务内完成。
pub struct AuthorizedResource<'a, R> {
    service: &'a ResourceService<R>,
    application: &'a ApplicationIR,
    resource: &'a str,
    role: Role,
    permission: &'a PermissionDefinition,
}

impl<'a, R: ResourceRepository> AuthorizedResource<'a, R> {
    pub async fn list(&self, query: ListQuery) -> Result<ListResult, ResourceError<R::Error>> {
        self.service
            .repository
            .list(self.resource, query)
            .await
            .map_err(ResourceError::Repository)
    }

    pub async fn get(&self, id: i64) -> Result<Record, ResourceError<R::Error>> {
        self.service
            .repository
            .get(self.resource, id)
            .await
            .map_err(ResourceError::Repository)?
            .ok_or(ResourceError::Missing)
    }

    pub fn prepare_create(
        self,
        input: Value,
    ) -> Result<AuthorizedCreate<'a, R>, ResourceError<R::Error>> {
        authorization::authorize_option_write(
            self.application,
            self.resource,
            &input,
            self.role,
            false,
        )?;
        Ok(AuthorizedCreate {
            access: self,
            input,
        })
    }

    pub fn prepare_update(
        self,
        input: Value,
    ) -> Result<AuthorizedUpdate<'a, R>, ResourceError<R::Error>> {
        authorization::authorize_option_write(
            self.application,
            self.resource,
            &input,
            self.role,
            true,
        )?;
        Ok(AuthorizedUpdate {
            access: self,
            input,
        })
    }

    pub async fn delete(&self, id: i64) -> Result<(), ResourceError<R::Error>> {
        if !self
            .service
            .repository
            .delete(self.resource, id)
            .await
            .map_err(ResourceError::Repository)?
        {
            return Err(ResourceError::Missing);
        }
        Ok(())
    }
}

pub struct AuthorizedCreate<'a, R> {
    access: AuthorizedResource<'a, R>,
    input: Value,
}

impl<R: ResourceRepository> AuthorizedCreate<'_, R> {
    pub async fn execute(self, owner: String) -> Result<Record, ResourceError<R::Error>> {
        let mut record = self
            .access
            .service
            .repository
            .create(self.access.resource, self.input, Some(owner))
            .await
            .map_err(ResourceError::Repository)?;
        if self.access.role != Role::Admin {
            record.retain(|key, _| {
                matches!(key.as_str(), "id" | "created_at" | "updated_at")
                    || self.access.permission.fields.contains(key)
            });
        }
        Ok(record)
    }
}

pub struct AuthorizedUpdate<'a, R> {
    access: AuthorizedResource<'a, R>,
    input: Value,
}

impl<R: ResourceRepository> AuthorizedUpdate<'_, R> {
    pub async fn execute(self, id: i64, owner: String) -> Result<Record, ResourceError<R::Error>> {
        self.access
            .service
            .repository
            .update(self.access.resource, id, self.input, Some(owner))
            .await
            .map_err(ResourceError::Repository)?
            .ok_or(ResourceError::Missing)
    }
}

pub struct OptionsField<'a, R> {
    service: &'a ResourceService<R>,
    application: &'a ApplicationIR,
    resource: &'a str,
    field_name: &'a str,
    field: &'a FieldDefinition,
    role: Role,
}

impl<'a, R: ResourceRepository> OptionsField<'a, R> {
    pub fn authorize(
        self,
        form: Option<&str>,
    ) -> Result<AuthorizedOptions<'a, R>, ResourceError<R::Error>> {
        let mut fields = Map::new();
        fields.insert(self.field.name.clone(), Value::Null);
        fields.extend(
            self.field
                .option_dependencies()
                .into_iter()
                .map(|name| (name.to_owned(), Value::Null)),
        );
        authorization::policy(
            self.application,
            self.resource,
            Operation::Create,
            self.role,
            form,
            Some(&fields),
        )?;
        Ok(AuthorizedOptions {
            service: self.service,
            resource: self.resource,
            field: self.field_name,
        })
    }
}

pub struct AuthorizedOptions<'a, R> {
    service: &'a ResourceService<R>,
    resource: &'a str,
    field: &'a str,
}

impl<R: ResourceRepository> AuthorizedOptions<'_, R> {
    pub async fn query(
        &self,
        query: OptionsQuery,
    ) -> Result<OptionsResult, ResourceError<R::Error>> {
        self.service
            .repository
            .options(self.resource, self.field, query)
            .await
            .map_err(ResourceError::Repository)
    }
}
