//!
//! # Global Context
//!
//! Global Context stores entities that persist through system operation.
//!
use std::sync::Arc;
use std::fmt::Debug;

use kf_socket::SharedSinkPool;
use kf_socket::SinkPool;
use types::SpuId;
use storage::ReplicaStorage;

use crate::config::SpuConfig;
use crate::controllers::leader_replica::SharedReplicaLeadersState;
use crate::controllers::follower_replica::FollowersState;
use crate::controllers::follower_replica::SharedFollowersState;
use crate::controllers::leader_replica::ReplicaLeadersState;
use super::spus::SharedSpuLocalStore;
use super::SharedReplicaLocalStore;
use super::spus::SpuLocalStore;
use super::replica::ReplicaStore;
use super::SharedSpuConfig;

#[derive(Debug)]
pub struct GlobalContext<S> {
    config: SharedSpuConfig,
    spu_localstore: SharedSpuLocalStore,
    replica_localstore: SharedReplicaLocalStore,
    leaders_state: SharedReplicaLeadersState<S>,
    followers_state: SharedFollowersState<S>,
    follower_sinks: SharedSinkPool<SpuId>
}

// -----------------------------------
// Global Contesxt - Implementation
// -----------------------------------

impl <S>GlobalContext<S> where S: ReplicaStorage + Debug {

    
    pub fn new_shared_context(spu_config: SpuConfig) -> Arc<Self>  {

        Arc::new(GlobalContext::new(spu_config))
    }
    

    pub fn new(spu_config: SpuConfig) -> Self 
    {
       
        GlobalContext {
            spu_localstore: SpuLocalStore::new_shared(),
            replica_localstore: ReplicaStore::new_shared(),
            config: Arc::new(spu_config),
            follower_sinks: SinkPool::new_shared(),
            leaders_state: ReplicaLeadersState::new_shared(),
            followers_state: FollowersState::new_shared()
        }
    }


    pub fn spu_localstore_owned(&self) -> SharedSpuLocalStore {
        self.spu_localstore.clone()
    }


    /// retrieves local spu id
    pub fn local_spu_id(&self) -> SpuId {
        self.config.id
    }

    pub fn spu_localstore(&self) -> &SpuLocalStore {
        &self.spu_localstore
    }

    pub fn replica_localstore(&self) -> &ReplicaStore {
        &self.replica_localstore
    }
 
    pub fn follower_sinks(&self) -> &SinkPool<SpuId> {
        &self.follower_sinks
    }

    pub fn followers_sink_owned(&self) -> SharedSinkPool<SpuId> {
        self.follower_sinks.clone()
    }

    pub fn leaders_state(&self) -> &ReplicaLeadersState<S> {
        &self.leaders_state
    }

    pub fn leader_state_owned(&self) -> SharedReplicaLeadersState<S> {
        self.leaders_state.clone()
    }

    pub fn followers_state(&self) -> &FollowersState<S> {
        &self.followers_state
    }

    pub fn followers_state_owned(&self) -> SharedFollowersState<S> {
        self.followers_state.clone()
    }

    pub fn config(&self) -> &SpuConfig {
        &self.config
    }

    pub fn config_owned(&self) -> SharedSpuConfig {
        self.config.clone()
    }


}
