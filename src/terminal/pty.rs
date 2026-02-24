use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

pub struct PtyHandle {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    pub child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    pub receiver: Receiver<Vec<u8>>,
    sender: Sender<Vec<u8>>,
}

impl PtyHandle {
    pub fn spawn(cols: u16, rows: u16) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair.slave.spawn_command(cmd)?;
        let child = Arc::new(Mutex::new(child));

        let master = pair.master;
        let writer = master.take_writer()?;

        let (sender, receiver) = bounded::<Vec<u8>>(256);
        let mut reader = master.try_clone_reader()?;
        let sender_clone = sender.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if sender_clone.send(data).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            master,
            writer,
            child,
            receiver,
            sender,
        })
    }

    pub fn write_bytes(&mut self, data: &[u8]) -> Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn try_recv_all(&self) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        while let Ok(chunk) = self.receiver.try_recv() {
            chunks.push(chunk);
        }
        chunks
    }
}
