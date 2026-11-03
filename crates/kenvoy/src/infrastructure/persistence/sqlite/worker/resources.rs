use serde_json::Value;

use crate::{
    application::resources::{
        ListQuery, ListResult, NewUpload, OptionsQuery, OptionsResult, Record, StoredFile,
    },
    domain::model::{ApplicationIR, UploadedFile},
};

use super::super::{DatabaseError, Result, migration, options, query, uploads};
use super::Database;

impl Database {
    pub async fn create(
        &self,
        resource: &str,
        input: Value,
        upload_owner: Option<String>,
    ) -> Result<Record> {
        let resource = resource.to_owned();
        self.submit(move |state| {
            let definition = state.resource(&resource)?.clone();
            query::create(
                &mut state.connection,
                &state.application,
                &definition,
                &input,
                upload_owner.as_deref(),
            )
        })
        .await
    }

    pub async fn get(&self, resource: &str, id: i64) -> Result<Option<Record>> {
        let resource = resource.to_owned();
        self.submit(move |state| query::get(&state.connection, state.resource(&resource)?, id))
            .await
    }

    pub async fn list(&self, resource: &str, query: ListQuery) -> Result<ListResult> {
        let resource = resource.to_owned();
        self.submit(move |state| {
            let definition = state.resource(&resource)?.clone();
            query::list(&mut state.connection, &definition, &query)
        })
        .await
    }

    pub async fn update(
        &self,
        resource: &str,
        id: i64,
        patch: Value,
        upload_owner: Option<String>,
    ) -> Result<Option<Record>> {
        let resource = resource.to_owned();
        self.submit(move |state| {
            let definition = state.resource(&resource)?.clone();
            query::update(
                &mut state.connection,
                &state.application,
                &definition,
                id,
                &patch,
                upload_owner.as_deref(),
            )
        })
        .await
    }

    pub async fn delete(&self, resource: &str, id: i64) -> Result<bool> {
        let resource = resource.to_owned();
        self.submit(move |state| {
            let definition = state.resource(&resource)?.clone();
            query::delete(&mut state.connection, &definition, id)
        })
        .await
    }

    pub async fn stage_upload(&self, upload: NewUpload) -> Result<UploadedFile> {
        self.submit(move |state| {
            let definition = state.resource(&upload.resource)?.clone();
            uploads::stage(&mut state.connection, &definition, upload, uploads::now()?)
        })
        .await
    }

    pub async fn file_info(&self, id: &str) -> Result<Option<StoredFile>> {
        let id = id.to_owned();
        self.submit(move |state| uploads::info(&state.connection, &id, uploads::now()?))
            .await
    }

    pub async fn file_content(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let id = id.to_owned();
        self.submit(move |state| uploads::content(&state.connection, &id, uploads::now()?))
            .await
    }

    pub async fn delete_upload(&self, id: &str, owner: &str) -> Result<bool> {
        let id = id.to_owned();
        let owner = owner.to_owned();
        self.submit(move |state| uploads::delete_staged(&state.connection, &id, &owner))
            .await
    }

    pub async fn options(
        &self,
        resource: &str,
        field: &str,
        query: OptionsQuery,
    ) -> Result<OptionsResult> {
        let resource = resource.to_owned();
        let field = field.to_owned();
        self.submit(move |state| {
            let definition = state.resource(&resource)?.clone();
            options::query(
                &mut state.connection,
                &state.application,
                &definition,
                &field,
                query,
            )
        })
        .await
    }

    /// 写队列内发布同一物理 Schema 的应用；正式库和快照均不在线生成迁移。
    /// HTTP/前端代际发布由宿主在调用前后另设屏障，不能仅依赖本方法。
    pub async fn replace_application(
        &self,
        application: ApplicationIR,
        confirmation: Option<String>,
    ) -> Result<migration::MigrationOutcome> {
        self.submit(move |state| {
            migration::schema_model(&application)?;
            if state.strict_schema
                && migration::schema_model(&state.application)? != migration::schema_model(&application)?
            {
                return Err(DatabaseError::InvalidState(
                    "持久化开发数据库不接受在线 Schema 变更；请固化迁移、停止服务后重新启动。开发快照请改用新的独立空路径".into(),
                ));
            }
            let generation = state
                .generation
                .checked_add(1)
                .ok_or_else(|| DatabaseError::InvalidState("数据库代际编号已耗尽".into()))?;
            let outcome = migration::apply_connection(
                &mut state.connection,
                &application,
                Some(&state.path),
                confirmation.as_deref(),
            )?;
            state.application = application;
            state.generation = generation;
            Ok(outcome)
        })
        .await
    }
}
