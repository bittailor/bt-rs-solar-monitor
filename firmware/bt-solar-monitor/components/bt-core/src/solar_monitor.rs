use embassy_futures::yield_now;
use embassy_sync::{blocking_mutex::raw::RawMutex, channel::Receiver};

use crate::sensor::ve_direct::Reading;

pub struct Runner<'a, M: RawMutex, const N: usize> {
    reading_receiver: Receiver<'a, M, Reading, N>,
}

pub fn new<'a, M: RawMutex, const N: usize>(reading_receiver: Receiver<'a, M, Reading, N>) -> Runner<'a, M, N> {
    Runner { reading_receiver }
}

impl<'a, M: RawMutex, const N: usize> Runner<'a, M, N> {
    pub async fn run(self) {
        loop {
            yield_now().await;
            let reading = self.reading_receiver.receive().await;
            info!("VE.Reading> {:?}", reading);
        }
    }
}
