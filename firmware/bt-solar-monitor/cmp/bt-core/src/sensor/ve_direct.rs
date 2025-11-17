use embedded_io_async::{Read, Write};
use heapless::{LinearMap, String};

#[derive(Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Reading {
    battery_voltage: f32, // V
    battery_current: f32, // I
    panel_voltage: f32,   // VPV
    panel_power: f32,     // PPV
    load_current: f32,    // IL
}

pub struct Runner<Stream: Read + Write> {
    frame_handler: FrameHandler<Stream>,
}

impl<Stream: Read + Write> Runner<Stream> {
    pub async fn run(mut self) {
        self.frame_handler.run().await;
    }
}

pub fn new<Stream: Read + Write>(stream: Stream) -> Runner<Stream> {
    Runner {
        frame_handler: FrameHandler::new(stream),
    }
}

const STRING_BUFFER_SIZE: usize = 64;
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

    async fn run(&mut self) {
        self.run_internal().await;
    }

    async fn run_internal(&mut self) {
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
                    info!("VE.Reading> {:?}", reading);
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
                    debug!("VE.Checksum> Valid => {} messages", messages.len());
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
}
