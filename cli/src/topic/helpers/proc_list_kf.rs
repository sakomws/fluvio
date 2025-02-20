//!
//! # Kafka - List Topic Processing
//!
//! Communicates with Kafka Controller to retrieve all Topics
//!

use std::net::SocketAddr;

use serde::Serialize;
use prettytable::Row;
use prettytable::row;
use prettytable::cell;

use crate::error::CliError;
use crate::common::OutputType;
use crate::common::{EncoderOutputHandler, TableOutputHandler};

use super::topic_metadata_kf::KfTopicMetadata;
use super::topic_metadata_kf::query_kf_topic_metadata;

use crate::topic::list::ListTopicsConfig;

// -----------------------------------
// Data Structures (Serializable)
// -----------------------------------

#[derive(Serialize, Debug)]
struct ListTopics {
    topics: Vec<KfTopicMetadata>,
}

// -----------------------------------
// Process Request
// -----------------------------------

// Retrieve and print topics in desired format
pub fn process_list_topics(
    server_addr: SocketAddr,
    list_topic_cfg: &ListTopicsConfig,
) -> Result<(), CliError> {
    let topics = query_kf_topic_metadata(server_addr, None)?;
    let list_topics = ListTopics { topics };
    process_server_response(&list_topics, &list_topic_cfg.output)
}

/// Process server based on output type
fn process_server_response(
    list_topics: &ListTopics,
    output_type: &OutputType,
) -> Result<(), CliError> {
    // expecting array with one or more elements
    if list_topics.topics.len() > 0 {
        if output_type.is_table() {
            list_topics.display_errors();
            list_topics.display_table(false);
        } else {
            list_topics.display_encoding(output_type)?;
        }
    } else {
        println!("No topics found");
    }
    Ok(())
}

// -----------------------------------
// Output Handlers
// -----------------------------------
impl TableOutputHandler for ListTopics {
    /// table header implementation
    fn header(&self) -> Row {
        row!["NAME", "INTERNAL", "PARTITIONS", "REPLICAS",]
    }

    /// return errors in string format
    fn errors(&self) -> Vec<String> {
        let mut errors = vec![];
        for topic_metadata in &self.topics {
            if let Some(error) = &topic_metadata.error {
                errors.push(format!(
                    "Topic '{}': {}",
                    topic_metadata.name,
                    error.to_sentence()
                ));
            }
        }
        errors
    }

    /// table content implementation
    fn content(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = vec![];
        for topic_metadata in &self.topics {
            if let Some(topic) = &topic_metadata.topic {
                rows.push(row![
                    l -> topic_metadata.name,
                    c -> topic.is_internal.to_string(),
                    c -> topic.partitions.to_string(),
                    c -> topic.replication_factor.to_string(),
                ]);
            }
        }
        rows
    }
}

impl EncoderOutputHandler for ListTopics {
    /// serializable data type
    type DataType = Vec<KfTopicMetadata>;

    /// serializable data to be encoded
    fn data(&self) -> &Vec<KfTopicMetadata> {
        &self.topics
    }
}
