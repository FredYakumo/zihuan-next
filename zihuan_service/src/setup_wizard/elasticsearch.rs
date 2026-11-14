//! Elasticsearch install logic for the detailed setup wizard: Compose service
//! rendering (with analyzer plugin bootstrapping), connection building,
//! retrieval index initialization, the binary install command, and
//! reachability checks.

use std::time::Duration;

use crate::setup_orchestrator::config_factory;
use crate::setup_wizard::{
    access_endpoint, compose_bind_mount, endpoint_host_port, wait_for_tcp, yaml_quote,
    DetailedComponentSource, DetailedSearchAuthMethod, DetailedSearchSetupConfig,
    DetailedSetupConfig, RETRIEVAL_SCHEMAS,
};
use zihuan_core::retrieval::RetrievalSchema;
use zihuan_core::storage::{
    ensure_elasticsearch_index, ConnectionAuthMethod, ConnectionConfig, ConnectionKind,
    ElasticsearchConnection, ElasticsearchRef,
};

/// Startup budget for [`verify`]. A wizard-installed Elasticsearch container
/// installs analyzer plugins before it even boots, so it gets the longest
/// budget.
const ELASTICSEARCH_VERIFY_TIMEOUT: Duration = Duration::from_secs(240);

/// Index definitions use the ICU and pinyin analyzers, which Elasticsearch only
/// provides through the `analysis-icu` and `analysis-pinyin` plugins. The official
/// image ships without them, so index creation fails with a 400 until they exist.
const ELASTICSEARCH_PINYIN_PLUGIN_URL: &str =
    "https://get.infini.cloud/elasticsearch/analysis-pinyin";

pub(crate) fn install_service(config: &DetailedSetupConfig) -> Option<&'static str> {
    (config.search.enabled
        && config.search.source == DetailedComponentSource::Install
        && config.search.search_type == "elasticsearch")
    .then_some("elasticsearch")
}

pub(crate) fn data_dir(config: &DetailedSetupConfig) -> Option<String> {
    (config.search.enabled
        && config.search.source == DetailedComponentSource::Install
        && config.search.search_type == "elasticsearch")
    .then(|| config.search.deployment.data_dir.clone())
}

pub(crate) fn compose_service(config: &DetailedSetupConfig) -> String {
    if !(config.search.enabled
        && config.search.source == DetailedComponentSource::Install
        && config.search.search_type == "elasticsearch")
    {
        return String::new();
    }
    let d = &config.search.deployment;
    format!("  elasticsearch:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:9200\"]\n    volumes: [{}]\n    environment:\n      discovery.type: single-node\n      xpack.security.enabled: 'true'\n      ELASTIC_PASSWORD: {}\n    {}\n", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, compose_bind_mount(&d.data_dir, "/usr/share/elasticsearch/data"), yaml_quote(config.search.password.as_deref().unwrap_or_default()), compose_command())
}

pub(crate) fn connection_kind(
    base_url: String,
    search: &DetailedSearchSetupConfig,
) -> ConnectionKind {
    ConnectionKind::Elasticsearch(ElasticsearchConnection {
        base_url,
        username: if search.source == DetailedComponentSource::Install {
            Some("elastic".to_string())
        } else if search.auth_method == DetailedSearchAuthMethod::Password {
            search.username.clone()
        } else {
            None
        },
        password: if search.auth_method == DetailedSearchAuthMethod::Password {
            search.password.clone()
        } else {
            None
        },
        // A wizard-installed Docker image creates the elastic user from
        // ELASTIC_PASSWORD but does not create an API key.
        api_key: if search.auth_method == DetailedSearchAuthMethod::ApiKey {
            search.api_key.clone()
        } else {
            None
        },
        auth_method: match search.auth_method {
            DetailedSearchAuthMethod::Password => ConnectionAuthMethod::Password,
            DetailedSearchAuthMethod::ApiKey => ConnectionAuthMethod::ApiKey,
        },
        vector_dimensions: search.vector_dimensions,
    })
}

/// The connection as published by the generated install command, pointing at
/// the target machine address when public access was requested.
pub(crate) fn remote_connection(config: &DetailedSetupConfig) -> Option<ConnectionConfig> {
    if !(config.search.enabled && config.search.search_type == "elasticsearch") {
        return None;
    }
    let base_url = access_endpoint(
        config,
        &config.search.base_url,
        "http",
        config.search.deployment.port,
    );
    Some(config_factory::build_connection(
        "setup-detailed-elasticsearch",
        "elasticsearch",
        connection_kind(base_url, &config.search),
    ))
}

/// Initializes every retrieval index the freshly installed Elasticsearch can
/// serve.
pub(crate) fn ensure_indices(elasticsearch: ElasticsearchConnection) -> Result<(), String> {
    for schema in RETRIEVAL_SCHEMAS {
        if !supports_schema(schema) {
            continue;
        }
        let reference = ElasticsearchRef::new(elasticsearch.clone(), schema)
            .map_err(|err| err.to_string())?;
        ensure_elasticsearch_index(&reference, true).map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// Elasticsearch has no QQ message index schema yet; Weaviate serves that schema.
fn supports_schema(schema: RetrievalSchema) -> bool {
    !matches!(schema, RetrievalSchema::QqMessage)
}

pub(crate) async fn verify(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.search.enabled && config.search.search_type == "elasticsearch" {
        let (host, port) = endpoint_host_port(&config.search.base_url, 9200)?;
        wait_for_tcp(&host, port, "search database", ELASTICSEARCH_VERIFY_TIMEOUT).await?;
    }
    Ok(())
}

pub(crate) fn binary_package(_package_manager: &str) -> Result<&'static str, String> {
    Ok("elasticsearch")
}

/// The copyable install command for an Elasticsearch package install on a
/// remote machine, including the analyzer plugins the index definitions need.
pub(crate) fn binary_install_command() -> String {
    format!(
        "# Run on the target Linux machine (dnf: sudo dnf install -y elasticsearch, macOS with Homebrew: brew install elasticsearch)\n\
sudo apt-get update && sudo apt-get install -y elasticsearch\n\
ES_VERSION=\"$(/usr/share/elasticsearch/bin/elasticsearch --version 2>/dev/null | sed -n 's/^Version: \\([0-9][0-9.]*\\).*/\\1/p' | head -n1)\"\n\
sudo /usr/share/elasticsearch/bin/elasticsearch-plugin install --batch analysis-icu\n\
sudo /usr/share/elasticsearch/bin/elasticsearch-plugin install --batch \"{ELASTICSEARCH_PINYIN_PLUGIN_URL}/$ES_VERSION\"\n\
sudo systemctl enable --now elasticsearch"
    )
}

/// Installs the analyzers required by the index definitions on every container
/// start, then hands over to the stock entrypoint. Already-installed plugins are
/// skipped, and an install failure only warns: the node still boots so the
/// remaining services stay reachable.
fn container_startup_script() -> String {
    format!(
        "ES_VERSION=\"$$(bin/elasticsearch --version 2>/dev/null | sed -n 's/^Version: \\([0-9][0-9.]*\\).*/\\1/p' | head -n1)\"\n\
if bin/elasticsearch-plugin list | grep -qx analysis-icu; then\n\
echo 'analysis-icu is already installed'\n\
else\n\
bin/elasticsearch-plugin install --batch analysis-icu || echo 'WARNING: failed to install analysis-icu; Elasticsearch will not create the Zihuan indices'\n\
fi\n\
if bin/elasticsearch-plugin list | grep -qx analysis-pinyin; then\n\
echo 'analysis-pinyin is already installed'\n\
else\n\
bin/elasticsearch-plugin install --batch \"{ELASTICSEARCH_PINYIN_PLUGIN_URL}/$${{ES_VERSION}}\" || echo 'WARNING: failed to install analysis-pinyin; Elasticsearch will not create the Zihuan indices'\n\
fi\n\
exec /usr/local/bin/docker-entrypoint.sh eswrapper"
    )
}

/// Compose `command:` block that runs [`container_startup_script`].
/// `$` is doubled because Compose interpolates the file before the container sees it.
fn compose_command() -> String {
    let mut block = String::from("command:\n      - /bin/bash\n      - -c\n      - |\n");
    for line in container_startup_script().lines() {
        block.push_str("        ");
        block.push_str(line);
        block.push('\n');
    }
    block
}
