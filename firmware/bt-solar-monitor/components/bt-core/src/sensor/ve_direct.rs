use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Channel, Receiver, Sender},
};
use embassy_time::Instant;
use embedded_io_async::{Read, Write};
use heapless::{LinearMap, String};

#[derive(Default, Debug)]
pub struct Averaging {
    sum: Reading,
    count: u32,
}

impl Averaging {
    pub fn add_reading(&mut self, reading: &Reading) {
        self.sum.battery_voltage += reading.battery_voltage;
        self.sum.battery_current += reading.battery_current;
        self.sum.panel_voltage += reading.panel_voltage;
        self.sum.panel_power += reading.panel_power;
        self.sum.load_current += reading.load_current;
        self.count += 1;
    }

    pub fn average(&mut self) -> Option<(Reading, u32)> {
        if self.count == 0 {
            None
        } else {
            let count = self.count;
            let reading = Some((
                Reading {
                    battery_voltage: self.sum.battery_voltage / count as f32,
                    battery_current: self.sum.battery_current / count as f32,
                    panel_voltage: self.sum.panel_voltage / count as f32,
                    panel_power: self.sum.panel_power / count as f32,
                    load_current: self.sum.load_current / count as f32,
                },
                count,
            ));
            self.sum = Reading::default();
            self.count = 0;
            reading
        }
    }
}

#[derive(Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Reading {
    pub battery_voltage: f32, // V
    pub battery_current: f32, // I
    pub panel_voltage: f32,   // VPV
    pub panel_power: f32,     // PPV
    pub load_current: f32,    // IL
}

pub struct Runner<'a, Stream: Read + Write, const N: usize> {
    frame_handler: FrameHandler<Stream>,
    averaging: Averaging,
    average_interval: embassy_time::Duration,
    rx: Sender<'a, NoopRawMutex, Reading, N>,
}

impl<Stream: Read + Write, const N: usize> Runner<'_, Stream, N> {
    pub async fn run(mut self) {
        loop {
            self.averaging_once().await;
        }
    }

    pub async fn averaging_once(&mut self) {
        let end = Instant::now() + self.average_interval;
        loop {
            let reading = self.frame_handler.read_next().await;
            self.averaging.add_reading(&reading);
            if Instant::now() >= end {
                if let Some((average, count)) = self.averaging.average() {
                    debug!("VE.Average> Over {} => {:?}", count, average);
                    self.rx.send(average).await;
                } else {
                    warn!("VE.Average> No readings collected during interval {}s", self.average_interval.as_secs());
                }
                self.averaging = Averaging::default();
                break;
            }
        }
    }
}

pub struct State<const N: usize> {
    channel: Channel<NoopRawMutex, Reading, N>,
}

impl<const N: usize> State<N> {
    pub fn new() -> Self {
        State { channel: Channel::new() }
    }
}

impl<const N: usize> Default for State<N> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn new<'a, Stream: Read + Write, const N: usize>(
    state: &'a mut State<N>,
    stream: Stream,
    average_interval: embassy_time::Duration,
) -> (Runner<'a, Stream, N>, Receiver<'a, NoopRawMutex, Reading, N>) {
    (
        Runner {
            frame_handler: FrameHandler::new(stream),
            averaging: Averaging::default(),
            average_interval,
            rx: state.channel.sender(),
        },
        state.channel.receiver(),
    )
}

const STRING_BUFFER_SIZE: usize = 32;
const MAX_MESSAGES: usize = 20;

struct FrameHandler<Stream: Read> {
    stream: Stream,
    checksum: Checksum,
}

impl<Stream: Read> FrameHandler<Stream> {
    fn new(stream: Stream) -> Self {
        FrameHandler {
            stream,
            checksum: Checksum::default(),
        }
    }

    pub async fn read_next(&mut self) -> Reading {
        loop {
            let values = self.run_once().await;
            match values {
                Ok(values) => {
                    let mut reading = Reading {
                        battery_voltage: 0.0,
                        battery_current: 0.0,
                        panel_voltage: 0.0,
                        panel_power: 0.0,
                        load_current: 0.0,
                    };
                    values.into_iter().for_each(|(label, value)| match label.as_str() {
                        "V" => {
                            if let Ok(mv) = value.as_str().parse::<u32>() {
                                reading.battery_voltage = mv as f32 / 1000.0;
                            }
                        }
                        "I" => {
                            if let Ok(ma) = value.as_str().parse::<i32>() {
                                reading.battery_current = ma as f32 / 1000.0;
                            }
                        }
                        "VPV" => {
                            if let Ok(mv) = value.as_str().parse::<u32>() {
                                reading.panel_voltage = mv as f32 / 1000.0;
                            }
                        }
                        "PPV" => {
                            if let Ok(w) = value.as_str().parse::<u32>() {
                                reading.panel_power = w as f32;
                            }
                        }
                        "IL" => {
                            if let Ok(ma) = value.as_str().parse::<i32>() {
                                reading.load_current = ma as f32 / 1000.0;
                            }
                        }
                        _ => {}
                    });
                    trace!("VE.Reading> {:?}", reading);
                    return reading;
                }
                Err(_) => {
                    warn!("Error reading VE frame");
                }
            }
        }
    }

    async fn run_once(&mut self) -> Result<LinearMap<String<STRING_BUFFER_SIZE>, String<STRING_BUFFER_SIZE>, MAX_MESSAGES>, ()> {
        while self.read_byte().await != b'\r' {
            self.checksum.clear();
        }
        self.checksum.add(b'\r');
        let mut messages = LinearMap::<String<STRING_BUFFER_SIZE>, String<STRING_BUFFER_SIZE>, MAX_MESSAGES>::new();
        loop {
            let byte = self.read_byte().await;
            self.checksum.add(byte);

            let label = self.read_label().await;
            if label == "Checksum" {
                let checksum_byte = self.read_byte().await;
                self.checksum.add(checksum_byte);
                if self.checksum.is_valid() {
                    trace!("VE.Checksum> Valid => {} messages", messages.len());
                    self.checksum.clear();
                    return Ok(messages);
                } else {
                    error!("VE.Checksum> Invalid ({:?})", self.checksum);
                    self.checksum.clear();
                    messages.clear();
                    return Err(());
                }
            } else {
                let value = self.read_value().await;
                trace!("VE.Message> Label: '{}', Value: '{}'", label, value);
                match messages.insert(label, value) {
                    Ok(_) => {}
                    Err(_) => {
                        error!("VE> Message map full, cannot insert new message");
                    }
                }
            }
        }
    }

    async fn read_label(&mut self) -> String<STRING_BUFFER_SIZE> {
        let mut label_buffer: heapless::Vec<u8, STRING_BUFFER_SIZE> = heapless::Vec::new();
        loop {
            let byte = self.read_byte().await;
            self.checksum.add(byte);
            if byte == b'\t' {
                trace!("Ve.RX label of lenght {}", label_buffer.len());
                break;
            } else {
                let _ = label_buffer.push(byte);
            }
        }
        match String::from_utf8(label_buffer) {
            Ok(label) => {
                trace!("VE.Label> {}", label.as_str());
                label
            }
            Err(_) => {
                error!("Invalid UTF-8 sequence");
                String::new()
            }
        }
    }

    async fn read_value(&mut self) -> String<STRING_BUFFER_SIZE> {
        let mut value_buffer: heapless::Vec<u8, STRING_BUFFER_SIZE> = heapless::Vec::new();
        loop {
            let byte = self.read_byte().await;
            self.checksum.add(byte);
            if byte == b'\r' {
                trace!("Ve.RX value of lenght {}", value_buffer.len());
                break;
            } else {
                let _ = value_buffer.push(byte);
            }
        }
        match String::from_utf8(value_buffer) {
            Ok(value) => {
                trace!("VE.Value> {}", value.as_str());
                value
            }
            Err(_) => {
                error!("Invalid UTF-8 sequence");
                String::new()
            }
        }
    }

    async fn read_byte(&mut self) -> u8 {
        loop {
            let mut byte_buffer = [0u8; 1];
            match self.stream.read(&mut byte_buffer).await {
                Ok(1) => {
                    let byte = byte_buffer[0];
                    trace!("read byte: {:02X}", byte);
                    return byte;
                }
                Ok(_) => continue,
                Err(_e) => warn!("Read error"),
            };
        }
    }
}

#[derive(Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
struct Checksum {
    value: u8,
}

impl Checksum {
    fn add(&mut self, byte: u8) {
        let before = self.value;
        self.value = self.value.wrapping_add(byte);
        trace!("Checksum add: {:02X} + {:02X} => {:02X}", before, byte, self.value);
    }

    fn is_valid(&self) -> bool {
        self.value == 0
    }

    fn clear(&mut self) {
        self.value = 0
    }
}

#[cfg(test)]
pub mod tests {
    use approx::assert_relative_eq;

    use super::*;

    #[tokio::test]
    async fn check_read_once() {
        let raw_data: [u8; _] = [
            0x0d, 0x0a, 0x50, 0x49, 0x44, 0x09, 0x30, 0x78, 0x32, 0x30, 0x33, 0x0d, 0x0a, 0x56, 0x09, 0x32, 0x36, 0x32, 0x30, 0x31, 0x0d, 0x0a, 0x49, 0x09,
            0x30, 0x0d, 0x0a, 0x50, 0x09, 0x30, 0x0d, 0x0a, 0x43, 0x45, 0x09, 0x30, 0x0d, 0x0a, 0x53, 0x4f, 0x43, 0x09, 0x31, 0x30, 0x30, 0x30, 0x0d, 0x0a,
            0x54, 0x54, 0x47, 0x09, 0x2d, 0x31, 0x0d, 0x0a, 0x41, 0x6c, 0x61, 0x72, 0x6d, 0x09, 0x4f, 0x46, 0x46, 0x0d, 0x0a, 0x52, 0x65, 0x6c, 0x61, 0x79,
            0x09, 0x4f, 0x46, 0x46, 0x0d, 0x0a, 0x41, 0x52, 0x09, 0x30, 0x0d, 0x0a, 0x42, 0x4d, 0x56, 0x09, 0x37, 0x30, 0x30, 0x0d, 0x0a, 0x46, 0x57, 0x09,
            0x30, 0x33, 0x30, 0x37, 0x0d, 0x0a, 0x43, 0x68, 0x65, 0x63, 0x6b, 0x73, 0x75, 0x6d, 0x09, 0xd8,
        ];
        let slice: &[u8] = &raw_data;
        let mut frame_handler = super::FrameHandler::new(slice);
        let values = frame_handler.run_once().await.unwrap();
        assert_eq!(values.get("PID").unwrap().as_str(), "0x203");
        assert_eq!(values.get("V").unwrap().as_str(), "26201");
        assert_eq!(values.get("P").unwrap().as_str(), "0");
        info!("Values count: {:?}", values.iter().count());
    }

    #[tokio::test]
    async fn check_read_twice() {
        let raw_data: [u8; _] = [
            0x0d, 0x0a, 0x50, 0x49, 0x44, 0x09, 0x30, 0x78, 0x32, 0x30, 0x33, 0x0d, 0x0a, 0x56, 0x09, 0x32, 0x36, 0x32, 0x30, 0x31, 0x0d, 0x0a, 0x49, 0x09,
            0x30, 0x0d, 0x0a, 0x50, 0x09, 0x30, 0x0d, 0x0a, 0x43, 0x45, 0x09, 0x30, 0x0d, 0x0a, 0x53, 0x4f, 0x43, 0x09, 0x31, 0x30, 0x30, 0x30, 0x0d, 0x0a,
            0x54, 0x54, 0x47, 0x09, 0x2d, 0x31, 0x0d, 0x0a, 0x41, 0x6c, 0x61, 0x72, 0x6d, 0x09, 0x4f, 0x46, 0x46, 0x0d, 0x0a, 0x52, 0x65, 0x6c, 0x61, 0x79,
            0x09, 0x4f, 0x46, 0x46, 0x0d, 0x0a, 0x41, 0x52, 0x09, 0x30, 0x0d, 0x0a, 0x42, 0x4d, 0x56, 0x09, 0x37, 0x30, 0x30, 0x0d, 0x0a, 0x46, 0x57, 0x09,
            0x30, 0x33, 0x30, 0x37, 0x0d, 0x0a, 0x43, 0x68, 0x65, 0x63, 0x6b, 0x73, 0x75, 0x6d, 0x09, 0xd8, 0x0d, 0x0a, 0x50, 0x49, 0x44, 0x09, 0x30, 0x78,
            0x32, 0x30, 0x33, 0x0d, 0x0a, 0x56, 0x09, 0x32, 0x36, 0x32, 0x30, 0x31, 0x0d, 0x0a, 0x49, 0x09, 0x30, 0x0d, 0x0a, 0x50, 0x09, 0x30, 0x0d, 0x0a,
            0x43, 0x45, 0x09, 0x30, 0x0d, 0x0a, 0x53, 0x4f, 0x43, 0x09, 0x31, 0x30, 0x30, 0x30, 0x0d, 0x0a, 0x54, 0x54, 0x47, 0x09, 0x2d, 0x31, 0x0d, 0x0a,
            0x41, 0x6c, 0x61, 0x72, 0x6d, 0x09, 0x4f, 0x46, 0x46, 0x0d, 0x0a, 0x52, 0x65, 0x6c, 0x61, 0x79, 0x09, 0x4f, 0x46, 0x46, 0x0d, 0x0a, 0x41, 0x52,
            0x09, 0x30, 0x0d, 0x0a, 0x42, 0x4d, 0x56, 0x09, 0x37, 0x30, 0x30, 0x0d, 0x0a, 0x46, 0x57, 0x09, 0x30, 0x33, 0x30, 0x37, 0x0d, 0x0a, 0x43, 0x68,
            0x65, 0x63, 0x6b, 0x73, 0x75, 0x6d, 0x09, 0xd8,
        ];
        let slice: &[u8] = &raw_data;
        let mut frame_handler = super::FrameHandler::new(slice);
        let values_1 = frame_handler.run_once().await.unwrap();
        let values_2 = frame_handler.run_once().await.unwrap();
        assert_eq!(values_1.get("PID").unwrap().as_str(), "0x203");
        assert_eq!(values_1.get("V").unwrap().as_str(), "26201");
        assert_eq!(values_1.get("P").unwrap().as_str(), "0");
        assert_eq!(values_2.get("PID").unwrap().as_str(), "0x203");
        assert_eq!(values_2.get("V").unwrap().as_str(), "26201");
        assert_eq!(values_2.get("P").unwrap().as_str(), "0");
    }

    #[tokio::test]
    async fn averaging() {
        let mut storage = Averaging::default();
        assert!(storage.average().is_none());

        storage.add_reading(&Reading {
            battery_voltage: 12.0,
            battery_current: 1.0,
            panel_voltage: 22.0,
            panel_power: 50.0,
            load_current: 0.8,
        });
        storage.add_reading(&Reading {
            battery_voltage: 12.0,
            battery_current: 1.0,
            panel_voltage: 18.0,
            panel_power: 52.0,
            load_current: 0.2,
        });

        let average = storage.average().unwrap();
        assert_eq!(average.1, 2);
        assert_relative_eq!(average.0.battery_voltage, 12.0);
        assert_relative_eq!(average.0.battery_current, 1.0);
        assert_relative_eq!(average.0.panel_voltage, 20.0);
        assert_relative_eq!(average.0.panel_power, 51.0);
        assert_relative_eq!(average.0.load_current, 0.5);

        assert!(storage.average().is_none());

        for i in 0..10 {
            storage.add_reading(&Reading {
                battery_voltage: 12.0 + i as f32,
                battery_current: 1.0 + i as f32,
                panel_voltage: 18.0 + i as f32,
                panel_power: 52.0 + i as f32,
                load_current: 0.2 + i as f32,
            });
        }
        let average = storage.average().unwrap();
        assert_eq!(average.1, 10);
        assert_relative_eq!(average.0.battery_voltage, 16.5);
        assert_relative_eq!(average.0.battery_current, 5.5);
        assert_relative_eq!(average.0.panel_voltage, 22.5);
        assert_relative_eq!(average.0.panel_power, 56.5);
        assert_relative_eq!(average.0.load_current, 4.7);

        assert!(storage.average().is_none());
    }
}
