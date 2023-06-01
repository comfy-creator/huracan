use figment::{
	providers::{Env, Format, Yaml},
	Figment,
};
use mongodb::{
	options::{ClientOptions, Compressor, ServerApi, ServerApiVersion},
	Database,
};

use crate::{_prelude::*, client::ClientPool};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
	#[serde(default)]
	pub name:                String,
	pub queuebuffers:        QueueBuffersConfig,
	pub workers:             WorkersConfig,
	pub objectqueries:       ObjectQueriesConfig,
	pub mongo:               MongoPipelineStepConfig,
	pub step1retries:        usize,
	pub step1retrytimeoutms: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectQueriesConfig {
	pub batchsize:          usize,
	pub batchwaittimeoutms: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MongoPipelineStepConfig {
	pub batchsize:          usize,
	pub batchwaittimeoutms: u64,
	pub retries:            usize,
	pub zstdlevel:          i32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueBuffersConfig {
	pub step1out:      usize,
	pub cpcompletions: usize,
	pub mongoinfactor: usize,
	pub last:          usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkersConfig {
	pub step1: Option<usize>,
	pub step2: Option<usize>,
	pub mongo: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MongoConfig {
	pub uri:            String,
	pub db:             String,
	pub collectionbase: String,
}

impl MongoConfig {
	pub async fn client(&self, pc: &MongoPipelineStepConfig) -> anyhow::Result<Database> {
		let mut client_options = ClientOptions::parse(&self.uri).await?;
		// use zstd compression for messages
		client_options.compressors = Some(vec![Compressor::Zstd { level: Some(pc.zstdlevel) }]);
		client_options.server_api = Some(ServerApi::builder().version(ServerApiVersion::V1).build());
		let client = mongodb::Client::with_options(client_options)?;
		Ok(client.database(&self.db))
	}
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PulsarConfig {
	pub url:         String,
	pub issuer:      String,
	pub credentials: String,
	pub audience:    String,
	pub topicbase:   String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
	pub level:  CLevel,
	pub ansi:   bool,
	pub filter: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcProviderConfig {
	pub url:               String,
	pub name:              String,
	pub objectsquerylimit: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuiConfig {
	pub testnet: Vec<RpcProviderConfig>,
	pub mainnet: Vec<RpcProviderConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
	pub env:                 String,
	pub net:                 String,
	pub rocksdbfile:         String,
	pub throughput:          PipelineConfig,
	pub lowlatency:          PipelineConfig,
	pub fallbehindthreshold: usize,
	pub pausepolloncatchup:  bool,
	pub pollintervalms:      u64,
	pub mongo:               MongoConfig,
	pub pulsar:              PulsarConfig,
	pub sui:                 SuiConfig,
	pub log:                 LogConfig,
}

impl AppConfig {
	pub fn new() -> anyhow::Result<Self> {
		let mut config: AppConfig =
			Figment::new().merge(Yaml::file("config.yaml")).merge(Env::prefixed("APP_").split("_")).extract()?;
		config.throughput.name = "throughput".into();
		config.lowlatency.name = "lowlatency".into();

		// FIXME validate that the directory is either empty, doesn't exist or contains ONLY rocksDB data files
		//			this is because we automatically remove the dir at runtime without further checks
		//			so if you've misconfigured this
		if config.rocksdbfile == "" || config.rocksdbfile == "/" {
			panic!("please set config.rocksdbfile to a new or empty or existing RocksDB data dir; it can and will be deleted at runtime, as needed!");
		}

		Ok(config)
	}

	pub async fn sui(&self) -> anyhow::Result<ClientPool> {
		let providers = if self.net == "testnet" {
			&self.sui.testnet
		} else if self.net == "mainnet" {
			&self.sui.mainnet
		} else {
			panic!("unknown net configuration: {} (expected: mainnet | testnet)", self.net);
		};
		if providers.is_empty() {
			panic!("no RPC providers configured for {}!", self.net);
		}
		Ok(ClientPool::new(providers.clone()).await?)
	}
}

// -- helpers

#[derive(Clone, Debug)]
pub struct CLevel(pub Level);

impl Deref for CLevel {
	type Target = Level;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<'de> Deserialize<'de> for CLevel {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CLevel, D::Error> {
		let s: String = Deserialize::deserialize(deserializer)?;
		Level::from_str(&s).map(CLevel).map_err(de::Error::custom)
	}
}
