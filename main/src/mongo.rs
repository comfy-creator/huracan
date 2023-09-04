use bson::doc;
use influxdb::InfluxDbWriteable;
use mongodb::Database;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::_prelude::*;
use crate::influx::{CheckpointError, CreateCheckpoint, IngestError};
use crate::influx::get_influx_timestamp_as_milliseconds;
use crate::conf::{get_config_singleton, get_influx_singleton};

#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
	// TODO mongo u64 issue
	pub _id:  u64,
	// marks the oldest checkpoint we need to look at, with the assurance that every checkpoint
	// older than this one has already been completed, even if we may not store that info otherwise
	pub stop: Option<bool>,
}

pub fn mongo_collection_name(cfg: &AppConfig, suffix: &str) -> String {
	format!("{}_{}_{}{}", cfg.env, cfg.net, cfg.mongo.collectionbase, suffix)
}

pub async fn mongo_checkpoint(cfg: &AppConfig, pc: &PipelineConfig, db: &Database, cp: CheckpointSequenceNumber) {
	let mut retries_left = pc.mongo.retries;
	let influx_client = get_influx_singleton();
	loop {
		if let Err(err) = db
			.run_command(
				doc! {
					// e.g. prod_testnet_objects_checkpoints
					"update": mongo_collection_name(&cfg, "_checkpoints"),
					"updates": vec![
						doc! {
							// FIXME how do we store a u64 in mongo? this will be an issue when the chain
							//		 has been running for long enough!
							"q": doc! { "_id": cp as i64 },
							"u": doc! { "_id": cp as i64 },
							"upsert": true,
						}
					]
				},
				None,
			)
			.await
		{
			warn!("failed saving checkpoint to mongo: {:?}", err);
			if retries_left > 0 {
				retries_left -= 1;
				let ts = get_influx_timestamp_as_milliseconds();
				let influx_item = CheckpointError {
					time: ts,
                    checkpoint_id: cp.to_string(),
				};
				let write_result = influx_client.query(influx_item).await;
				match write_result {
					Ok(string) => debug!(string),
					Err(error) => warn!("Could not write to influx: {}", error),
				}
				continue
			}
			error!(error = ?err, "checkpoint {} fully completed, but could not save checkpoint status to mongo!", cp);
		}
		// At this point, we have successfully saved the checkpoint to MongoDB.
		let ts = get_influx_timestamp_as_milliseconds();
		let influx_item = CreateCheckpoint {
			time: ts,
			checkpoint_id: cp.to_string(),
		}.into_query("sui_object_deleted");
		let write_result = influx_client.query(influx_item).await;
		match write_result {
			Ok(string) => debug!(string),
			Err(error) => warn!("Could not write to influx: {}", error),
		}
		break
	}
}
