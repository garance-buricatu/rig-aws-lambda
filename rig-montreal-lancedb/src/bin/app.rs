use std::{env, str::FromStr};

use lambda_runtime::{run, service_fn, tracing::Level, Error, LambdaEvent};
use lancedb::Connection;
use rig::{
    completion::Prompt,
    providers::{
        self,
        openai::{GPT_4O, TEXT_EMBEDDING_ADA_002},
    },
};
use rig_lancedb::{LanceDbVectorStore, SearchParams};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Event {
    prompt: String,
}

#[derive(Serialize)]
struct AgentResponse {
    response: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let env_log_level = std::env::var("LOG_LEVEL").unwrap_or("info".to_string());

    tracing_subscriber::fmt()
        .with_max_level(Level::from_str(&env_log_level).unwrap())
        // disable printing the name of the module in every log line.
        .with_target(false)
        // disabling time is handy because CloudWatch will add the ingestion time.
        .without_time()
        .init();

    // Initialize the OpenAI client
    let openai_client = providers::openai::Client::new(
        &env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set"),
    );

    // Initialize LanceDb client on EFS mount target
    // Use `/mnt/efs` if data is stored on EFS
    // Use `/tmp` if data is stored on local disk in lambda
    // Use S3 uri if data is stored in S3
    let db = lancedb::connect("/mnt/efs").execute().await?;

    run(service_fn(|request: LambdaEvent<Event>| {
        handler(request, &openai_client, &db)
    }))
    .await
}

async fn handler(
    request: LambdaEvent<Event>,
    openai_client: &providers::openai::Client,
    db: &Connection,
) -> Result<AgentResponse, Error> {
    let model = openai_client.embedding_model(TEXT_EMBEDDING_ADA_002);

    let table = db.open_table("montreal_data").execute().await?;

    // Define search_params params that will be used by the vector store to perform the vector search.
    let search_params = SearchParams::default().distance_type(lancedb::DistanceType::Cosine);
    let index = LanceDbVectorStore::new(table, model, "id", search_params).await?;

    // Create agent with a single context prompt
    let spotify_agent = openai_client
        .agent(GPT_4O)
        .dynamic_context(2, index)
        .build();

    // Prompt the agent and print the response
    let response = spotify_agent.prompt(&request.payload.prompt).await?;

    Ok(AgentResponse { response })
}
