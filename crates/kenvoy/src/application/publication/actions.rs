use serde_json::Value;

use crate::{
    application::execution::{FunctionExecutor, JsError},
    domain::{
        authorization::{self, AuthorizationError},
        model::Role,
    },
};

use super::Generation;

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error(transparent)]
    Authorization(#[from] AuthorizationError),
    #[error("函数运行时尚未准备完成")]
    NotReady,
    #[error(transparent)]
    Execution(#[from] JsError),
}

pub struct AuthorizedAction<'a, E> {
    handler: &'a str,
    executor: Option<&'a E>,
}

impl<E: FunctionExecutor> Generation<E> {
    pub fn action(&self, name: &str, role: Role) -> Result<AuthorizedAction<'_, E>, ActionError> {
        let action = authorization::action(&self.application, name, role)?;
        Ok(AuthorizedAction {
            handler: &action.handler,
            executor: self.javascript.as_ref(),
        })
    }
}

impl<E: FunctionExecutor> AuthorizedAction<'_, E> {
    pub async fn call(&self, args: Vec<Value>) -> Result<Value, ActionError> {
        let executor = self.executor.ok_or(ActionError::NotReady)?;
        Ok(executor.call(self.handler, args).await?)
    }
}
