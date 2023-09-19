//! The local transport.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json;

use std::marker::PhantomData;

use crate::transport::{TransportConfig, TransportChannel};
use crate::{transport_config, LocationSet};

use crate::core::{HList, Portable, Transport};
use crate::utils::queue::BlockingQueue;

type QueueMap = HashMap<String, HashMap<String, BlockingQueue<String>>>;

/// The local transport.
///
/// This transport uses a blocking queue to allow for communication between threads. Each location must be executed in its thread.
///
/// Unlike network-based transports, all locations must share the same `LocalTransport` instance. The struct implements `Clone` so that it can be shared across threads.
#[derive(Clone)]
pub struct LocalTransport<L: HList> {
    internal_locations: Vec<String>,
    // queue_map: Arc<QueueMap>,
    location_set: PhantomData<L>,
    transport_channel: TransportChannel<L, Arc<QueueMap>>,
}


impl<L: HList> LocalTransport<L> {
    pub fn transport_channel() -> TransportChannel<L, Arc<QueueMap>> {
        let mut queue_map: QueueMap = HashMap::new();
        for sender in L::to_string_list() {
            let mut n = HashMap::new();
            for receiver in L::to_string_list() {
                n.insert(receiver.to_string(), BlockingQueue::new());
            }
            queue_map.insert(sender.to_string(), n);
        }

        TransportChannel {
            location_set: PhantomData,
            queue_map: Arc::new(queue_map.into()),
        }
    }

    /// Creates a new `LocalTransport` instance from a list of locations.
    pub fn new(_local_config: &TransportConfig<L, (), ()>, transport_channel: TransportChannel<L, Arc<QueueMap>>) -> Self {
        let locations_list = L::to_string_list();

        let mut locations_vec = Vec::new();
        for loc in locations_list.clone() {
            locations_vec.push(loc.to_string());
        }
        
        LocalTransport {
            internal_locations: locations_vec,
            location_set: PhantomData,
            transport_channel,
        }
    }
}

impl<L: HList> Transport<L> for LocalTransport<L> {
    fn locations(&self) -> Vec<String> {
        return self.internal_locations.clone();
    }

    fn send<T: Portable>(&self, from: &str, to: &str, data: &T) -> () {
        let data = serde_json::to_string(data).unwrap();
        self.transport_channel.queue_map
            .get(from)
            .unwrap()
            .get(to)
            .unwrap()
            .push(data)
    }

    fn receive<T: Portable>(&self, from: &str, at: &str) -> T {
        let data = self.transport_channel.queue_map.get(from).unwrap().get(at).unwrap().pop();
        serde_json::from_str(&data).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ChoreographyLocation;
    use std::thread;

    #[derive(ChoreographyLocation)]
    struct Alice;

    #[derive(ChoreographyLocation)]
    struct Bob;

    #[test]
    fn test_local_transport() {
        let v = 42;
        
        let transport_channel = LocalTransport::<LocationSet!(Alice, Bob)>::transport_channel();

        let mut handles = Vec::new();
        {
            let config = transport_config!(
                Alice,
                Alice: (),
                Bob: ()
            );
            let transport_channel = transport_channel.clone();

            let transport = LocalTransport::new(&config, transport_channel);
            handles.push(thread::spawn(move || {
                transport.send::<i32>(Alice::name(), Bob::name(), &v);
            }));
        }
        {
            let config = transport_config!(
                Bob,
                Alice: (),
                Bob: ()
            );
            // let transport = transport.clone();
            let transport_channel = transport_channel.clone();
            let transport = LocalTransport::new(&config, transport_channel);
            handles.push(thread::spawn(move || {
                let v2 = transport.receive::<i32>(Alice::name(), Bob::name());
                assert_eq!(v, v2);
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
