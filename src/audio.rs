use std::collections::VecDeque;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use axum::body::Bytes;
use tokio::io::AsyncWriteExt;
use tokio::time::{MissedTickBehavior, interval};

const FRAME_BYTES: usize = 1920; // 20 ms, PCM s16le, mono, 48 kHz
const SILENCE: [u8; FRAME_BYTES] = [0; FRAME_BYTES];
const START_FRAMES: usize = 2;
const MAX_FRAMES: usize = 8;
const LEASE: Duration = Duration::from_secs(2);
pub const BATCH_BYTES: usize = FRAME_BYTES * 2;

#[derive(Default)]
pub struct AudioBuffer {
    sender: Option<(String, Instant)>,
    frames: VecDeque<Bytes>,
    playing: bool,
}

impl AudioBuffer {
    pub fn push(&mut self, sender: &str, chunk: Bytes) -> bool {
        let now = Instant::now();
        if let Some((current, last)) = &self.sender {
            if current != sender && now.duration_since(*last) < LEASE {
                return false;
            }
            if current != sender || now.duration_since(*last) >= LEASE {
                self.frames.clear();
                self.playing = false;
            }
        }
        self.sender = Some((sender.to_owned(), now));

        for start in (0..chunk.len()).step_by(FRAME_BYTES) {
            if self.frames.len() == MAX_FRAMES {
                self.frames.pop_front();
            }
            self.frames
                .push_back(chunk.slice(start..start + FRAME_BYTES));
        }
        true
    }

    pub fn stop(&mut self, sender: &str) {
        if self
            .sender
            .as_ref()
            .is_some_and(|(current, _)| current == sender)
        {
            *self = Self::default();
        }
    }

    fn next_frame(&mut self) -> Option<Bytes> {
        if !self.playing {
            self.playing = self.frames.len() >= START_FRAMES;
        }
        if self.playing {
            let frame = self.frames.pop_front();
            self.playing = frame.is_some();
            return frame;
        }
        None
    }
}

pub struct VirtualMic {
    sink: String,
    modules: Vec<String>,
}

impl VirtualMic {
    pub fn create() -> Result<Self> {
        let suffix = std::process::id();
        let mut mic = Self {
            sink: format!("webmic_output_{suffix}"),
            modules: Vec::new(),
        };
        mic.modules.push(load_module(&[
            "module-null-sink",
            &format!("sink_name={}", mic.sink),
            r#"sink_properties="device.description=WebMic-Transport priority.session=1""#,
            "rate=48000",
            "channels=1",
            "channel_map=mono",
        ])?);
        mic.modules.push(load_module(&[
            "module-remap-source",
            &format!("master={}.monitor", mic.sink),
            &format!("source_name=webmic_mic_{suffix}"),
            r#"source_properties="device.description=WebMic priority.session=1""#,
            "channels=1",
            "channel_map=mono",
            "master_channel_map=mono",
        ])?);
        Ok(mic)
    }

    pub async fn run(&self, buffer: Arc<Mutex<AudioBuffer>>) -> Result<()> {
        let mut child = tokio::process::Command::new("pw-cat")
            .args([
                "--playback",
                "--raw",
                "--target",
                &self.sink,
                "--rate",
                "48000",
                "--channels",
                "1",
                "--format",
                "s16",
                "--latency",
                "40ms",
                "-",
            ])
            .stdin(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("pw-cat starten")?;
        let mut stdin = child.stdin.take().context("Kein pw-cat-Eingang")?;
        let playback = async {
            let mut pace = interval(Duration::from_millis(20));
            pace.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                pace.tick().await;
                let frame = buffer.lock().unwrap().next_frame();
                stdin
                    .write_all(frame.as_deref().unwrap_or(&SILENCE))
                    .await?;
            }
            #[allow(unreachable_code)]
            Ok::<(), std::io::Error>(())
        };
        tokio::select! {
            result = playback => result.context("PipeWire-Audio übertragen"),
            status = child.wait() => bail!("pw-cat beendet: {}", status?),
        }
    }
}

impl Drop for VirtualMic {
    fn drop(&mut self) {
        for module in self.modules.iter().rev() {
            let _ = Command::new("pactl")
                .args(["unload-module", module])
                .status();
        }
    }
}

fn load_module(args: &[&str]) -> Result<String> {
    let result = Command::new("pactl")
        .arg("load-module")
        .args(args)
        .output()
        .context("pactl starten. WebMic benötigt pipewire-pulse.")?;
    if !result.status.success() {
        bail!(
            "PipeWire-Modul {}: {}",
            args[0],
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    let id = String::from_utf8(result.stdout)?.trim().to_owned();
    id.parse::<u32>().context("Ungültige PipeWire-Modul-ID")?;
    Ok(id)
}
