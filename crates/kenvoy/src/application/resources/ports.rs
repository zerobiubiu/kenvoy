use serde_json::Value;

use crate::domain::{
    model::{ApplicationIR, UploadedFile},
    validation::ValidationErrors,
};

use super::{ListQuery, ListResult, NewUpload, OptionsQuery, OptionsResult, Record, StoredFile};

#[derive(Debug)]
pub enum RepositoryFailure {
    Validation(ValidationErrors),
    UnknownResource,
    InvalidQuery,
    Conflict,
    Busy,
    Closed,
    WorkerLost,
    UploadLimit,
    Internal,
}

pub trait ResourceRepository: Clone + Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    fn classify(error: Self::Error) -> RepositoryFailure;
    fn create(
        &self,
        resource: &str,
        input: Value,
        upload_owner: Option<String>,
    ) -> impl Future<Output = Result<Record, Self::Error>> + Send;
    fn get(
        &self,
        resource: &str,
        id: i64,
    ) -> impl Future<Output = Result<Option<Record>, Self::Error>> + Send;
    fn list(
        &self,
        resource: &str,
        query: ListQuery,
    ) -> impl Future<Output = Result<ListResult, Self::Error>> + Send;
    fn update(
        &self,
        resource: &str,
        id: i64,
        patch: Value,
        upload_owner: Option<String>,
    ) -> impl Future<Output = Result<Option<Record>, Self::Error>> + Send;
    fn delete(
        &self,
        resource: &str,
        id: i64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn options(
        &self,
        resource: &str,
        field: &str,
        query: OptionsQuery,
    ) -> impl Future<Output = Result<OptionsResult, Self::Error>> + Send;
    fn stage_upload(
        &self,
        upload: NewUpload,
    ) -> impl Future<Output = Result<UploadedFile, Self::Error>> + Send;
    fn file_info(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<StoredFile>, Self::Error>> + Send;
    fn file_content(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, Self::Error>> + Send;
    fn delete_upload(
        &self,
        id: &str,
        owner: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn generation(&self) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    fn replace_model(
        &self,
        application: ApplicationIR,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
