//! The HTTP transport.

use std::thread;
use std::{collections::HashMap, sync::Arc};

use std::marker::PhantomData;

use retry::{
    delay::{jitter, Fixed},
    retry,
};
use tiny_http::Server;
use ureq::{Agent, AgentBuilder};

use crate::transport::{TransportChannel, TransportConfig};
#[cfg(test)]
use crate::{transport_config, LocationSet};

use crate::{
    core::{ChoreographyLocation, Equal, HList, Member, Portable, Transport},
    utils::queue::BlockingQueue,
};

type QueueMap = HashMap<String, BlockingQueue<String>>;

/// The header name for the source location.
const HEADER_SRC: &str = "X-CHORUS-SOURCE";

/// The HTTP transport.
pub struct HttpTransport<L: HList> {
    config: HashMap<String, (String, u16)>,
    agent: Agent,
    server: Arc<Server>,
    join_handle: Option<thread::JoinHandle<()>>,
    location_set: PhantomData<L>,
    transport_channel: TransportChannel<L, QueueMap>,
}

impl<L: HList> HttpTransport<L> {
    /// Creates a `TransportChannel`.
    pub fn transport_channel() -> TransportChannel<L, QueueMap> {
        let queue_map = {
            let mut m = HashMap::new();
            for loc in L::to_string_list() {
                m.insert(loc.to_string(), BlockingQueue::new());
            }
            m
        };

        TransportChannel {
            location_set: PhantomData,
            queue_map: Arc::new(queue_map.into()),
        }
    }

    /// Creates a new `HttpTransport` instance from the projection target and a configuration.
    pub fn new<C: ChoreographyLocation, L2: HList, Index, IndexList>(
        http_config: &TransportConfig<L2, (String, u16), C, (String, u16)>,
        transport_channel: TransportChannel<L, QueueMap>,
    ) -> Self
    where
        C: Member<L, Index>,
        L2: Equal<L, IndexList>,
    {
        let info = &http_config.info;

        let (_, (hostname, port)) = &http_config.target_info;
        let server = Arc::new(Server::http(format!("{}:{}", hostname, port)).unwrap());
        let join_handle = Some({
            let server = server.clone();
            let queue_map = transport_channel.queue_map.clone();

            thread::spawn(move || {
                for mut request in server.incoming_requests() {
                    let mut body = String::new();
                    request
                        .as_reader()
                        .read_to_string(&mut body)
                        .expect("Failed to read body");
                    let mut headers = request.headers().iter();
                    let src = headers.find(|header| header.field.equiv(HEADER_SRC));
                    if let Some(src) = src {
                        let src = &src.value;
                        queue_map.get(src.as_str()).unwrap().push(body);
                        request
                            .respond(tiny_http::Response::from_string("OK").with_status_code(200))
                            .unwrap();
                    } else {
                        request
                            .respond(
                                tiny_http::Response::from_string("Bad Request")
                                    .with_status_code(400),
                            )
                            .unwrap();
                    }
                }
            })
        });

        let agent = AgentBuilder::new().build();

        Self {
            config: info.clone(),
            agent,
            join_handle,
            server,
            location_set: PhantomData,
            transport_channel,
        }
    }
}

impl<L: HList> Drop for HttpTransport<L> {
    fn drop(&mut self) {
        self.server.unblock();
        self.join_handle.take().map(thread::JoinHandle::join);
    }
}

impl<L: HList> Transport<L> for HttpTransport<L> {
    fn locations(&self) -> Vec<String> {
        Vec::from_iter(self.config.keys().map(|s| s.clone()))
    }

    fn send<V: Portable>(&self, from: &str, to: &str, data: &V) -> () {
        let (hostname, port) = self.config.get(to).unwrap();
        retry(Fixed::from_millis(1000).map(jitter), move || {
            self.agent
                .post(format!("http://{}:{}", hostname, port).as_str())
                .set(HEADER_SRC, from)
                .send_string(serde_json::to_string(data).unwrap().as_str())
        })
        .unwrap();
    }

    fn receive<V: Portable>(&self, from: &str, _at: &str) -> V {
        let str = self.transport_channel.queue_map.get(from).unwrap().pop();
        serde_json::from_str(&str).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread::{self, sleep};
    use std::time::Duration;

    use super::*;
    use crate::core::ChoreographyLocation;

    #[derive(ChoreographyLocation)]
    struct Alice;

    #[derive(ChoreographyLocation)]
    struct Bob;

    #[test]
    fn test_http_transport() {
        let v = 42;

        let (signal, wait) = mpsc::channel::<()>();

        let transport_channel = HttpTransport::<LocationSet!(Alice, Bob)>::transport_channel();

        let mut handles = Vec::new();
        {
            let config = transport_config!(
                Alice,
                Alice: ("0.0.0.0".to_string(), 9010),
                Bob: ("localhost".to_string(), 9011)
            );

            let transport_channel = transport_channel.clone();

            handles.push(thread::spawn(move || {
                wait.recv().unwrap(); // wait for Bob to start
                let transport = HttpTransport::new(&config, transport_channel);
                transport.send::<i32>(Alice::name(), Bob::name(), &v);
            }));
        }
        {
            let config = transport_config!(
                Bob,
                Alice: ("localhost".to_string(), 9010),
                Bob: ("0.0.0.0".to_string(), 9011)
            );

            let transport_channel = transport_channel.clone();
            handles.push(thread::spawn(move || {
                let transport = HttpTransport::new(&config, transport_channel);
                signal.send(()).unwrap();
                let v2 = transport.receive::<i32>(Alice::name(), Bob::name());
                assert_eq!(v, v2);
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_http_transport_retry() {
        let v = 42;
        let (signal, wait) = mpsc::channel::<()>();

        let transport_channel = HttpTransport::<LocationSet!(Alice, Bob)>::transport_channel();

        let mut handles = Vec::new();
        {
            let config = transport_config!(
                Alice,
                Alice: ("0.0.0.0".to_string(), 9020),
                Bob: ("localhost".to_string(), 9021)
            );

            let transport_channel = transport_channel.clone();

            handles.push(thread::spawn(move || {
                signal.send(()).unwrap();
                let transport = HttpTransport::new(&config, transport_channel);
                transport.send::<i32>(Alice::name(), Bob::name(), &v);
            }));
        }
        {
            let config = transport_config!(
                Bob,
                Alice: ("localhost".to_string(), 9020),
                Bob: ("0.0.0.0".to_string(), 9021)
            );

            let transport_channel = transport_channel.clone();

            handles.push(thread::spawn(move || {
                // wait for Alice to start, which forces Alice to retry
                wait.recv().unwrap();
                sleep(Duration::from_millis(100));
                let transport = HttpTransport::new(&config, transport_channel);
                let v2 = transport.receive::<i32>(Alice::name(), Bob::name());
                assert_eq!(v, v2);
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
