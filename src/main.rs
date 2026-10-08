use actix_multipart::{Field, Multipart};
use actix_web::{web, App, Error, HttpResponse, HttpServer};
use anyhow::Result;
use format_yaml_with_ollama::format_yaml;
use futures_util::stream::StreamExt;
use futures_util::TryStreamExt;
use graflog::{app_log, init_logging};
use std::env;
use std::io::Write;
use std::path::Path;
use uuid::Uuid;
use graflog::LogOption;

mod extract_yaml;
mod format_yaml_with_ollama;
mod load_prompt;
mod model_config;
mod models;
mod yaml_validator;

use model_config::ModelConfigCache;

struct AppState {
    template_path: String,
    reference_data_template_path: String,
    system_prompt_path: String,
    user_prompt_path: String,
    model_cache: ModelConfigCache,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load environment variables at startup
    dotenv::dotenv().ok();

    if env::var("AI_UPLOADER_PORT").is_err() {
        eprintln!("Error: AI_UPLOADER_PORT environment variable is required");
        std::process::exit(1);
    }
    if env::var("LOG_PATH_API0").is_err() {
        eprintln!("Error: LOG_PATH_API0 environment variable is required");
        std::process::exit(1);
    }

    let log_path = env::var("LOG_PATH_API0").unwrap_or_else(|_| "/var/log/api0.log".to_string());
       init_logging!(&log_path, "api0", "ai-uploader", &[
    LogOption::Debug,
    LogOption::RocketOff
]);

    // Parse command line arguments - optional "server" subcommand
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] != "server" {
        app_log!(info, "Usage: {} [server]", args[0]);
        std::process::exit(1);
    }

    app_log!(info, "Starting YAML formatter HTTP service");

    let port = env::var("AI_UPLOADER_PORT")
        .or_else(|_| env::var("PORT"))
        .unwrap_or_else(|_| "6666".to_string())
        .parse::<u16>()
        .map_err(|e| {
            app_log!(error, "Invalid port number: {}", e);
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid port number")
        })?;

    // Get base path from environment or use current directory
    let base_path = if let Ok(config_path) = env::var("CONFIG_PATH") {
        // Production path provided via environment variable
        if config_path.ends_with(".yaml") {
            std::path::Path::new(&config_path)
                .parent()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        } else {
            config_path
        }
    } else {
        // Development: use current directory
        ".".to_string()
    };

    // Define file paths relative to base path
    let template_file_path = format!("{}/template.yaml", base_path);
    let reference_data_template_path = format!("{}/template_ref_data.yaml", base_path);
    let system_prompt_path = format!("{}/prompt/system_prompt.txt", base_path);
    let user_prompt_path = format!("{}/prompt/user_prompt.txt", base_path);

    app_log!(info, "Using base path: {}", base_path);
    app_log!(info, "Template file: {}", template_file_path);
    app_log!(info, "System prompt: {}", system_prompt_path);
    app_log!(info, "User prompt: {}", user_prompt_path);
    app_log!(info, "Starting server on port: {}", port);

    // Ensure the prompt directory exists
    let prompt_dir = format!("{}/prompt", base_path);
    if !Path::new(&prompt_dir).exists() {
        std::fs::create_dir_all(&prompt_dir).expect("Failed to create prompt directory");
        app_log!(info, "Created prompt directory");
    }

    // Check if prompt files exist
    for (path, name) in [
        (&system_prompt_path, "system prompt"),
        (&user_prompt_path, "user prompt"),
        (&user_prompt_path, "user prompt"),
        (&template_file_path, "template file"),
        (&reference_data_template_path, "reference data template file"),
    ] {
        if !Path::new(path).exists() {
            app_log!(error, "{} file not found at {}", name, path);
            panic!("Missing {} file", name);
        }
    }

    let store_url = std::env::var("STORE_URL").ok();
    app_log!(info, "Store URL for model config: {:?}", store_url);

    let app_state = web::Data::new(AppState {
        template_path: template_file_path,
        reference_data_template_path,
        system_prompt_path,
        user_prompt_path,
        model_cache: ModelConfigCache::new(store_url),
    });

    // Start HTTP server with dynamic port
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/format-yaml", web::post().to(format_yaml_handler))
            .route("/format-reference-data", web::post().to(format_reference_data_handler))
            .route("/health", web::get().to(health_check))
            .route("/providers", web::get().to(providers))
    })
    .bind(format!("0.0.0.0:{}", port))?
    .run()
    .await
}

async fn health_check() -> HttpResponse {
    HttpResponse::Ok().body("Service is running")
}

/// Which providers this deployment can actually serve.
///
/// "Supported" and "usable" are different questions, and conflating them is why
/// a super admin could select DeepSeek on the dashboard, be told it was saved,
/// and have every upload fail with an error visible only in the logs — the
/// provider list described what the binary can do, not what this machine has a
/// key for.
///
/// Only this process can answer it: the keys are in its environment and nowhere
/// else. Names of the variables are returned so a missing one can be named in
/// the message; no key material is exposed.
async fn providers() -> HttpResponse {
    // The same table the dispatch in format_yaml_handler uses, so the two
    // cannot drift apart.
    let providers: Vec<_> = format_yaml_with_ollama::PROVIDERS
        .iter()
        .map(|p| {
            let configured = std::env::var(p.key_var).map(|v| !v.trim().is_empty()).unwrap_or(false);
            serde_json::json!({
                "provider": p.id,
                "configured": configured,
                "env_var": p.key_var,
            })
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({ "providers": providers }))
}

/// The provider, model and key the store sent with the file — what the super
/// admin chose in the dashboard. The key may be absent when none was set
/// there; the provider's key in this process's environment is used instead.
struct Chosen {
    config: model_config::AiConfig,
    api_key: Option<String>,
}

async fn read_text(mut field: Field) -> Result<String, Error> {
    let mut bytes = web::BytesMut::new();
    while let Some(chunk) = field.next().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(String::from_utf8_lossy(&bytes).trim().to_string())
}

/// The uploaded file, saved to a temporary path, and the store's choice of
/// provider if it sent one. A store from before the dashboard setting sends
/// only the file.
async fn read_upload(mut multipart: Multipart) -> Result<(Option<String>, Option<Chosen>), Error> {
    let (mut provider, mut model, mut api_key, mut input_path) = (None, None, None, None);
    while let Some(field) = multipart.try_next().await? {
        match field.name() {
            Some("provider") => provider = Some(read_text(field).await?),
            Some("model") => model = Some(read_text(field).await?),
            Some("api_key") => api_key = Some(read_text(field).await?).filter(|k| !k.is_empty()),
            Some("file") => {
                input_path = Some(save_field(field).await?);
                break;
            }
            _ => {}
        }
    }
    let chosen = provider.zip(model).map(|(provider, model)| Chosen {
        config: model_config::AiConfig { provider, model },
        api_key,
    });
    Ok((input_path, chosen))
}

async fn save_field(field: Field) -> Result<String, Error> {
    let content_disposition = field.content_disposition();
    let filename = content_disposition
        .and_then(|cd| cd.get_filename())
        .unwrap_or("upload.txt");
    let filepath = format!(
        "/tmp/{}-{}",
        Uuid::new_v4(),
        sanitize_filename::sanitize(filename)
    );

    app_log!(debug, "Saving uploaded file to {}", filepath);

    let mut temp_file = std::fs::File::create(&filepath)?;
    let mut bytes = web::BytesMut::new();

    let mut field_stream = field;
    while let Some(chunk) = field_stream.next().await {
        let data = chunk?;
        bytes.extend_from_slice(&data);
        temp_file.write_all(&data)?;
    }

    temp_file.flush()?;
    Ok(filepath)
}

async fn format_yaml_handler(
    multipart: Multipart,
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    app_log!(info, "Processing uploaded file");
    let (input_path, chosen) = read_upload(multipart).await?;

    let input_file_path = input_path.ok_or_else(|| {
        app_log!(error, "No file was uploaded");
        actix_web::error::ErrorBadRequest("No file was uploaded")
    })?;

    app_log!(info, "Processing file: {}", input_file_path);

    let ai_config = match &chosen {
        Some(c) => c.config.clone(),
        None => app_state.model_cache.get_config().await,
    };
    let api_key = chosen.and_then(|c| c.api_key);
    app_log!(info, "Using provider={} model={} key_from={}", ai_config.provider, ai_config.model,
        if api_key.is_some() { "store" } else { "environment" });

    let result = format_yaml(
        format_yaml_with_ollama::provider(&ai_config.provider),
        &input_file_path, &app_state.template_path,
        &app_state.system_prompt_path, &app_state.user_prompt_path,
        &ai_config.model,
        api_key.as_deref(),
    ).await;

    match result {
        Ok(formatted_yaml) => {
            app_log!(info, "Successfully formatted YAML");
            if let Err(e) = std::fs::remove_file(&input_file_path) {
                app_log!(error, "Failed to remove temporary input file: {}", e);
            }
            Ok(HttpResponse::Ok()
                .content_type("application/yaml")
                .append_header((
                    "Content-Disposition",
                    "attachment; filename=\"formatted_output.yaml\"",
                ))
                .body(formatted_yaml))
        }
        Err(e) => {
            app_log!(error, "Error formatting YAML: {}", e);
            Ok(HttpResponse::InternalServerError().body(format!("Error: {}", e)))
        }
    }
}

async fn format_reference_data_handler(
    multipart: Multipart,
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    app_log!(info, "Processing uploaded reference data file");
    let (input_path, chosen) = read_upload(multipart).await?;

    let input_file_path = input_path.ok_or_else(|| {
        app_log!(error, "No file was uploaded");
        actix_web::error::ErrorBadRequest("No file was uploaded")
    })?;

    app_log!(info, "Processing file: {}", input_file_path);

    let ai_config = match &chosen {
        Some(c) => c.config.clone(),
        None => app_state.model_cache.get_config().await,
    };
    let api_key = chosen.and_then(|c| c.api_key);
    app_log!(info, "Using provider={} model={} key_from={}", ai_config.provider, ai_config.model,
        if api_key.is_some() { "store" } else { "environment" });

    let result = format_yaml(
        format_yaml_with_ollama::provider(&ai_config.provider),
        &input_file_path, &app_state.reference_data_template_path,
        &app_state.system_prompt_path, &app_state.user_prompt_path,
        &ai_config.model,
        api_key.as_deref(),
    ).await;

    match result {
        Ok(formatted_yaml) => {
            app_log!(info, "Successfully formatted reference data");
            if let Err(e) = std::fs::remove_file(&input_file_path) {
                app_log!(error, "Failed to remove temporary input file: {}", e);
            }
            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .append_header((
                    "Content-Disposition",
                    "attachment; filename=\"reference_data.json\"",
                ))
                .body(formatted_yaml))
        }
        Err(e) => {
            app_log!(error, "Error formatting reference data: {}", e);
            if let Err(cleanup_err) = std::fs::remove_file(&input_file_path) {
                app_log!(error, "Failed to remove temporary input file: {}", cleanup_err);
            }
            Ok(HttpResponse::InternalServerError().body(format!("Error: {}", e)))
        }
    }
}
