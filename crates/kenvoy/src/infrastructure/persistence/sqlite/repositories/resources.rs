use serde_json::Value;

use crate::{
    application::resources::{
        ListQuery, ListResult, NewUpload, OptionsQuery, OptionsResult, Record, RepositoryFailure,
        ResourceRepository, StoredFile,
    },
    domain::model::{ApplicationIR, UploadedFile},
};

use super::super::{Database, DatabaseError, Result};

impl ResourceRepository for Database {
    type Error = DatabaseError;

    fn classify(error: Self::Error) -> RepositoryFailure {
        match error {
            DatabaseError::Validation(errors) => RepositoryFailure::Validation(errors),
            DatabaseError::UnknownResource(_) => RepositoryFailure::UnknownResource,
            DatabaseError::InvalidQuery(_) => RepositoryFailure::InvalidQuery,
            DatabaseError::Conflict => RepositoryFailure::Conflict,
            DatabaseError::Busy => RepositoryFailure::Busy,
            DatabaseError::Closed => RepositoryFailure::Closed,
            DatabaseError::WorkerLost => RepositoryFailure::WorkerLost,
            DatabaseError::UploadLimit => RepositoryFailure::UploadLimit,
            _ => RepositoryFailure::Internal,
        }
    }

    async fn create(
        &self,
        resource: &str,
        input: Value,
        upload_owner: Option<String>,
    ) -> Result<Record> {
        Database::create(self, resource, input, upload_owner).await
    }

    async fn get(&self, resource: &str, id: i64) -> Result<Option<Record>> {
        Database::get(self, resource, id).await
    }

    async fn list(&self, resource: &str, query: ListQuery) -> Result<ListResult> {
        Database::list(self, resource, query).await
    }

    async fn update(
        &self,
        resource: &str,
        id: i64,
        patch: Value,
        upload_owner: Option<String>,
    ) -> Result<Option<Record>> {
        Database::update(self, resource, id, patch, upload_owner).await
    }

    async fn delete(&self, resource: &str, id: i64) -> Result<bool> {
        Database::delete(self, resource, id).await
    }

    async fn options(
        &self,
        resource: &str,
        field: &str,
        query: OptionsQuery,
    ) -> Result<OptionsResult> {
        Database::options(self, resource, field, query).await
    }

    async fn stage_upload(&self, upload: NewUpload) -> Result<UploadedFile> {
        Database::stage_upload(self, upload).await
    }

    async fn file_info(&self, id: &str) -> Result<Option<StoredFile>> {
        Database::file_info(self, id).await
    }

    async fn file_content(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Database::file_content(self, id).await
    }

    async fn delete_upload(&self, id: &str, owner: &str) -> Result<bool> {
        Database::delete_upload(self, id, owner).await
    }

    async fn generation(&self) -> Result<u64> {
        Ok(Database::info(self).await?.generation)
    }

    async fn replace_model(&self, application: ApplicationIR) -> Result<()> {
        Database::replace_application(self, application, None)
            .await
            .map(|_| ())
    }
}
