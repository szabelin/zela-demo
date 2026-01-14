use serde::{Deserialize, Serialize};
use zela_std::{CustomProcedure, RpcError, zela_custom_procedure};

#[derive(Deserialize, Debug)]
pub struct Input {}

#[derive(Serialize)]
pub struct Output {}

pub struct PriorityFees;
impl CustomProcedure for PriorityFees {
    type Params = Input;
    type ErrorData = ();
    type SuccessData = Output;

    // Run method is the entry point of every custom procedure
    // It will be called once for each incoming request.
    async fn run(params: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
        log::info!("Hello world!");

        Ok(Output {})
    }

    const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
}
zela_custom_procedure!(PriorityFees);
