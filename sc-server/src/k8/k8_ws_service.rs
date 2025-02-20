//!
//! # Update KV Store with SPU status (online/offline)
//!
use std::fmt::Debug;
use std::fmt::Display;
use std::convert::Into;
use std::io::Error as IoError;
use std::io::ErrorKind;


use futures::future::BoxFuture;
use futures::future::FutureExt;
use log::trace;
use log::warn;
use log::debug;
use serde::de::DeserializeOwned;
use serde::Serialize;

use metadata::topic::TopicSpec;
use metadata::partition::PartitionSpec;
use metadata::spu::SpuSpec; 
use k8_metadata::core::metadata::InputK8Obj;

use types::log_on_err;

use k8_metadata::core::Spec as K8Spec;
use k8_metadata::core::metadata::UpdateK8ObjStatus;
use k8_client::K8Client;

use crate::ScServerError;
use crate::core::Spec;
use crate::core::common::KVObject;
use crate::core::WSUpdateService;
use crate::core::common::WSAction;
use super::SharedK8Client;

#[derive(Clone)]
pub struct K8WSUpdateService(SharedK8Client);



impl K8WSUpdateService {

    pub fn new(client: SharedK8Client) -> Self {
         Self(client)
    }


    pub fn client(&self) -> &K8Client {
        &self.0
    }

    pub fn own_client(&self) -> SharedK8Client {
        self.0.clone()
    }


     pub async fn add<S>(
        &self,
        value: KVObject<S>,
    ) -> Result<(), ScServerError>  
         where S: Spec + Debug,
             S::Status: Debug + PartialEq,
             S::Key: Display + Debug ,
            <S as Spec>::K8Spec: Debug + From<S>  + Default + DeserializeOwned + Serialize + Clone ,
            <<S as Spec>::K8Spec as K8Spec>::Status: Default + Debug + DeserializeOwned + Serialize + Clone

    {

        debug!("Adding: {}:{}",S::LABEL,value.key());
        trace!("adding KV {:#?} to k8 kv", value);

        let (key, spec,kv_ctx) = value.parts();
        let k8_spec: S::K8Spec = spec.into();
       
        if let Some(item_ctx) = kv_ctx.item_ctx {
            
            let new_k8 = InputK8Obj::new(k8_spec,item_ctx.into());

            self.0
                .apply(new_k8)
                .await.map(|_| ()).map_err(|err|err.into())

        } else if let Some(ref parent_metadata) = kv_ctx.parent_ctx {

            let item_name = key.to_string();

            let new_k8 = InputK8Obj::new(k8_spec,parent_metadata.make_child_input_metadata::<<<S as Spec>::Owner as Spec>::K8Spec>(item_name));

            self.0
                .apply(new_k8)
                .await.map(|_| ()).map_err(|err| err.into())
        } else {
            Err(IoError::new(
                ErrorKind::Other,
                format!("{} add failed - no item or context {}",S::LABEL,key)
            ).into())
        }

        
    }

    /// only update the status
    async fn update_status<S>(
        &self,
        value: KVObject<S>,
    ) -> Result<(), ScServerError> 
         where S: Spec + Debug, 
            S::Key: Debug + Display,
            S::Status: Debug + Display + Into< <<S as Spec>::K8Spec as K8Spec>::Status>,
            <S as Spec>::K8Spec: Debug + Default + Serialize + DeserializeOwned,
            <<S as Spec>::K8Spec as K8Spec>::Status:  Default + Debug + Serialize + DeserializeOwned

    {

        debug!("K8 Update Status: {} key: {} value: {}",S::LABEL,value.key(),value.status);
        trace!("status update: {:#?}",value.status);

        let k8_status: <<S as Spec>::K8Spec as K8Spec>::Status = value.status().clone().into();

        if let Some(ref kv_ctx) = value.kv_ctx().item_ctx {

            let k8_input: UpdateK8ObjStatus<S::K8Spec,<<S as Spec>::K8Spec as K8Spec>::Status> = UpdateK8ObjStatus {
                api_version: S::K8Spec::api_version(),
                kind: S::K8Spec::kind(),
                metadata: kv_ctx.clone().into(),
                status: k8_status,
                ..Default::default()
            };

        
            self.0
                .update_status(&k8_input)
                .await
                .map(|_| ())
                .map_err(|err| err.into())
        } else {
            Err(IoError::new(
                ErrorKind::Other,
                "KVS update failed - missing  KV ctx".to_owned(),
            ).into())
        }
    

    }

    /// update both spec and status
    async fn update_spec<S>(
        &self,
        value: KVObject<S>,
    ) -> Result<(), ScServerError> 
         where 
            S: Spec + Debug + Into<<S as Spec>::K8Spec>,
            S::Key: Debug + Display ,
            S::Status: Debug + Into< <<S as Spec>::K8Spec as K8Spec>::Status>,
            <S as Spec>::K8Spec: Debug + Default + Serialize + DeserializeOwned + Clone,
            <<S as Spec>::K8Spec as K8Spec>::Status:  Default + Debug + Serialize + DeserializeOwned + Clone

    {

        debug!("K8 Update Spec: {} key: {}",S::LABEL,value.key());
        trace!("K8 Update Spec: {:#?}",value);
        let k8_spec: <S as Spec>::K8Spec = value.spec().clone().into(); 

        if let Some(ref kv_ctx) = value.kv_ctx().item_ctx {

             trace!("updating spec: {:#?}",k8_spec);

            let k8_input: InputK8Obj<S::K8Spec> = InputK8Obj {
                api_version: S::K8Spec::api_version(),
                kind: S::K8Spec::kind(),
                metadata: kv_ctx.clone().into(),
                spec: k8_spec,
                ..Default::default()
            };

        
            self.0
                .apply(k8_input)
                .await
                .map(|_| ())
                .map_err(|err| err.into())
        } else {
             Err(IoError::new(
                ErrorKind::Other,
                "KVS update failed - missing  KV ctx".to_owned(),
            ).into())
        }
    

    }


    async fn inner_process<S>(&self,action: WSAction<S>) -> Result<(), ScServerError> 

        where 
            S: Spec + Debug, 
            S::Key: Display + Debug ,
            S::Status: Debug + PartialEq + Display,
            <S as Spec>::K8Spec: From<S> + Clone + Debug + Default + Serialize + DeserializeOwned ,
            <<S as Spec>::K8Spec as K8Spec>::Status: From<S::Status> + Clone + Default + Debug + Serialize + DeserializeOwned

    {

        match action {
            WSAction::Add(value) => log_on_err!(self.add(value).await),
            WSAction::UpdateStatus(value) => log_on_err!(self.update_status(value).await),
            WSAction::UpdateSpec(value) => log_on_err!(self.update_spec(value).await),
            WSAction::Delete(_key) => warn!("delete not yet implemente")
        }
            
        Ok(())
    }
}

impl WSUpdateService for K8WSUpdateService {

    type ResponseFuture = BoxFuture<'static, Result<(), ScServerError>>;

    fn update_spu(&self,ws_actions: WSAction<SpuSpec>) -> Self::ResponseFuture {
        
        let service = self.clone();
        async move {
            service.inner_process(ws_actions).await?;
            Ok(())
        }.boxed()
    }

    fn update_topic(&self,ws_actions: WSAction<TopicSpec>) -> Self::ResponseFuture {

        let service = self.clone();
        async move {
            service.inner_process(ws_actions).await?;
            Ok(())
        }.boxed()
    }

    fn update_partition(&self,ws_actions: WSAction<PartitionSpec>) -> Self::ResponseFuture {
    
        let service = self.clone();
        async move {
            service.inner_process(ws_actions).await?;
            Ok(())
        }.boxed()
    }
    
}