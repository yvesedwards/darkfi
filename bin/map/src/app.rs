use crate::{
    list::NodeIdList,
    node::{NodeInfo, NodeInfoView},
};
use rand::Rng;
use smol::Timer;
use std::{collections::HashMap, time::Duration};

#[derive(Clone)]
pub struct App {
    pub node_list: NodeIdList,
    pub node_info: NodeInfoView,
}

impl App {
    pub fn new() -> App {
        // node info struct w fields
        // make 10 node info
        let infos = vec![NodeInfo {
            id: "sodisofjhosd".to_string(),
            connections: 10,
            is_active: true,
            last_message: "hey how are you?".to_string(),
        }];

        let node_info = NodeInfoView::new(infos.clone());

        let ids = vec![infos[0].id.clone()];
        let node_list = NodeIdList::new(ids);
        App { node_list, node_info }
    }

    // TODO: implement this
    //async fn sleep(self, dur: Duration) {
    //    Timer::after(dur).await;
    //}

    //pub async fn update(mut self) {
    //    self.node_list.nodes.insert("New node joined".to_string(), "".to_string());
    //    //self.sleep(Duration::from_secs(2)).await;
    //}
}
