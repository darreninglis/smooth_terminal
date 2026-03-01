pub mod cell;
pub mod grid;
pub mod parser;
pub mod pty;
pub mod url;

use anyhow::Result;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

use grid::TerminalGrid;
use parser::VtePerformer;
use pty::PtyHandle;

pub struct Terminal {
    pub grid: Arc<Mutex<TerminalGrid>>,
    pub pty: PtyHandle,
    parser: vte::Parser,
    performer: VtePerformer,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize, cwd: Option<&PathBuf>) -> Result<Self> {
        let grid = Arc::new(Mutex::new(TerminalGrid::new(cols, rows)));
        let pty = PtyHandle::spawn(cols as u16, rows as u16, cwd)?;
        let performer = VtePerformer::new(grid.clone());
        let parser = vte::Parser::new();
        Ok(Self { grid, pty, parser, performer })
    }

    /// Drain PTY output and process through VTE parser. Call every frame.
    pub fn drain_pty_output(&mut self) {
        let chunks = self.pty.try_recv_all();
        for chunk in chunks {
            self.parser.advance(&mut self.performer, &chunk);
        }
    }

    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        self.pty.write_bytes(data)
    }

    pub fn resize(&mut self, cols: usize, rows: usize) -> Result<()> {
        self.grid.lock().resize(cols, rows);
        self.pty.resize(cols as u16, rows as u16)?;
        Ok(())
    }

    /// Returns true if the shell process has exited.
    pub fn is_pty_dead(&self) -> bool {
        if let Ok(mut child) = self.pty.child.lock() {
            matches!(child.try_wait(), Ok(Some(_)))
        } else {
            false
        }
    }
}
