use crossbeam_channel::{bounded, unbounded, Sender, Receiver};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;

/// Unique ID for each channel
pub type ChannelId = u64;

/// Type-erased channel storage
struct ChannelStorage {
    /// Sender side (type-erased)
    sender: Box<dyn std::any::Any + Send>,
    /// Receiver side (type-erased)
    receiver: Box<dyn std::any::Any + Send>,
}

lazy_static! {
    /// Global registry of all channels
    static ref CHANNEL_REGISTRY: Arc<Mutex<HashMap<ChannelId, ChannelStorage>>> = {
        Arc::new(Mutex::new(HashMap::new()))
    };

    /// Channel ID counter
    static ref CHANNEL_ID_COUNTER: Arc<Mutex<u64>> = {
        Arc::new(Mutex::new(1))
    };
}

/// Generic channel wrapper
pub struct Channel<T> {
    pub id: ChannelId,
    pub sender: Sender<T>,
    pub receiver: Receiver<T>,
}

impl<T: Send + 'static> Channel<T> {
    /// Create a new bounded channel with the given capacity
    pub fn new_bounded(capacity: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        let id = Self::allocate_id();

        // Store in registry
        let storage = ChannelStorage {
            sender: Box::new(sender.clone()),
            receiver: Box::new(receiver.clone()),
        };

        CHANNEL_REGISTRY.lock().unwrap().insert(id, storage);

        Channel { id, sender, receiver }
    }

    /// Create a new unbounded channel
    pub fn new_unbounded() -> Self {
        let (sender, receiver) = unbounded();
        let id = Self::allocate_id();

        // Store in registry
        let storage = ChannelStorage {
            sender: Box::new(sender.clone()),
            receiver: Box::new(receiver.clone()),
        };

        CHANNEL_REGISTRY.lock().unwrap().insert(id, storage);

        Channel { id, sender, receiver }
    }

    /// Allocate a new unique channel ID
    fn allocate_id() -> ChannelId {
        let mut counter = CHANNEL_ID_COUNTER.lock().unwrap();
        let id = *counter;
        *counter += 1;
        id
    }

    /// Send a value to the channel
    pub fn send(&self, value: T) -> Result<(), String> {
        self.sender.send(value)
            .map_err(|_| "Channel is closed".to_string())
    }

    /// Receive a value from the channel (blocking)
    /// Returns None if the channel is closed
    pub fn recv(&self) -> Option<T> {
        self.receiver.recv().ok()
    }

    /// Close the channel by dropping the sender
    pub fn close(&mut self) {
        // In crossbeam-channel, we can't explicitly close, but we can drop all senders
        // When all senders are dropped, recv() will return None
        // For now, we'll remove from registry which should drop the stored sender
        CHANNEL_REGISTRY.lock().unwrap().remove(&self.id);
    }
}

/// Get a channel from the registry by ID
pub fn get_channel<T: Clone + Send + 'static>(id: ChannelId) -> Option<Channel<T>> {
    let registry = CHANNEL_REGISTRY.lock().unwrap();
    let storage = registry.get(&id)?;

    // Downcast the sender and receiver
    let sender = storage.sender.downcast_ref::<Sender<T>>()?.clone();
    let receiver = storage.receiver.downcast_ref::<Receiver<T>>()?.clone();

    Some(Channel { id, sender, receiver })
}
