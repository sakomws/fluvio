//!
//! # Replica Messages
//!
//! Replicas are sent from SC to all live SPUs that participate in this replica group.
//! This message is sent for any changes in the live replica group.
//!
//! UPDATE/DEL operation is computed at sender by comparing KV notification with
//! internal metadata cache. Receiver translates UPDATE operations into an ADD/DEL
//! operation the comparing message with internal metadata.
//!
use std::fmt;


use kf_protocol::derive::{Decode, Encode};

use types::SpuId;

use metadata::partition::ReplicaKey;

use super::MsgType;
use super::Message;

pub type ReplicaMsg = Message<Replica>;

// -----------------------------------
// Data Structures
// -----------------------------------

#[derive(Decode, Encode, Debug, PartialEq, Clone, Default)]
pub struct ReplicaMsgs {
    pub messages: Vec<ReplicaMsg>,
}



impl fmt::Display for ReplicaMsgs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"[")?;
        for replica in &self.messages {
             write!(f,"{},",replica)?;
        }
        write!(f,"]")
       
    }
}


// -----------------------------------
// ReplicaMsgs
// -----------------------------------

impl ReplicaMsgs {
    pub fn new(replica_msgs: Vec<ReplicaMsg>) -> Self {
        ReplicaMsgs {
            messages: replica_msgs,
        }
    }

    pub fn push(&mut self, msg: ReplicaMsg) {
        self.messages.push(msg);
    }
}



#[derive(Decode, Encode, Debug, PartialEq, Clone, Default)]
pub struct Replica {
    pub id: ReplicaKey,
    pub leader: SpuId,
    pub replicas: Vec<SpuId>,
}


impl fmt::Display for Replica {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"{} leader: {} replicas: [",self.id,self.leader)?;
        for replica in &self.replicas {
            write!(f,"{},",replica)?;
        }
        write!(f,"]")
    }
}


// -----------------------------------
// ReplicaMsg
// -----------------------------------

impl ReplicaMsg {
    pub fn create_delete_msg(name: ReplicaKey, leader: SpuId) -> Self {
        ReplicaMsg {
            header: MsgType::DELETE,
            content: Replica::new(name, leader, vec![]),
        }
    }
}

// -----------------------------------
// Replica
// -----------------------------------

impl Replica {
    pub fn new(id: ReplicaKey, leader: SpuId, replicas: Vec<SpuId>) -> Self {
        Replica {
            id,
            leader,
            replicas,
        }
    }
}
