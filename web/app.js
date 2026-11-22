const token = location.hash.slice(1);
const toggle = document.querySelector("#toggle");
const status = document.querySelector("#status");
const indicator = document.querySelector("#indicator");
const level = document.querySelector("#level");

let current = null;

function show(message, state = "") {
  status.textContent = message;
  indicator.dataset.state = state;
}

function stop(connection, message = "Bereit", state = "") {
  if (current !== connection) return;

  current = null;
  connection.request?.abort();
  connection.input?.disconnect();
  connection.recorder?.disconnect();
  connection.stream?.getTracks().forEach((track) => track.stop());
  connection.audio?.close().catch(() => {});
  if (connection.sent) {
    fetch("/audio", { method: "DELETE", headers: connection.headers, keepalive: true })
      .catch(() => {});
  }

  level.value = 0;
  toggle.textContent = "Mikrofon starten";
  toggle.disabled = false;
  show(message, state);
}

async function send(connection) {
  if (connection.request || current !== connection) return;

  while (connection.pending && current === connection) {
    const packet = connection.pending;
    connection.pending = null;
    const controller = new AbortController();
    connection.request = controller;
    const deadline = setTimeout(() => controller.abort(), 2000);
    try {
      connection.sent = true;
      const result = await fetch("/audio", {
        method: "POST",
        headers: connection.headers,
        body: packet,
        signal: controller.signal,
      });
      if (current !== connection) return;
      if (result.status === 409) {
        stop(connection, "Ein anderes Gerät sendet bereits.", "error");
        return;
      }
      if (result.status === 403) {
        stop(connection, "Öffne den aktuellen Link aus dem PC-Terminal.", "error");
        return;
      }
      if (!result.ok) throw new Error(`HTTP ${result.status}`);
      show("Verbunden · Mikrofon aktiv", "live");
    } catch {
      stop(connection, "Verbindung zum PC unterbrochen.", "error");
    } finally {
      clearTimeout(deadline);
      connection.request = null;
    }
  }
}

async function start() {
  const connection = {
    headers: {
      "Content-Type": "application/octet-stream",
      "X-WebMic-Token": token,
      "X-WebMic-Session": crypto.randomUUID(),
    },
  };
  current = connection;
  toggle.disabled = true;
  show("Mikrofon wird geöffnet …");

  try {
    connection.audio = new AudioContext({ sampleRate: 48000 });
    if (connection.audio.sampleRate !== 48000) {
      throw new Error("Dieser Browser konnte 48 kHz nicht öffnen.");
    }
    const resume = connection.audio.resume();
    // Permission prompts can suspend the audio context briefly.
    resume.catch(() => {});
    connection.stream = await navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });
    if (current !== connection) {
      connection.stream.getTracks().forEach((track) => track.stop());
      return;
    }
    for (const track of connection.stream.getTracks()) {
      track.addEventListener("ended", () => stop(connection, "Mikrofonverbindung beendet.", "error"));
    }
    await resume;
    await connection.audio.audioWorklet.addModule("/worklet.js");
    if (current !== connection) return;

    connection.input = connection.audio.createMediaStreamSource(connection.stream);
    connection.recorder = new AudioWorkletNode(connection.audio, "webmic-recorder");
    const silent = connection.audio.createGain();
    silent.gain.value = 0;
    connection.input.connect(connection.recorder).connect(silent).connect(connection.audio.destination);
    connection.recorder.port.onmessage = ({ data }) => {
      if (current !== connection) return;
      level.value = Math.min(1, data.peak * 2);
      connection.pending = data.pcm;
      void send(connection);
    };

    show("Verbindung zum PC wird aufgebaut …");
    toggle.textContent = "Mikrofon trennen";
    toggle.disabled = false;
  } catch (error) {
    const message = error.name === "NotAllowedError"
      ? "Erlaube Safari den Zugriff auf dein Mikrofon."
      : `Mikrofon konnte nicht gestartet werden. ${error.message}`;
    stop(connection, message, "error");
  }
}

if (!/^[a-f0-9]{32}$/.test(token)) {
  show("Öffne den vollständigen Link aus dem PC-Terminal.", "error");
  toggle.disabled = true;
} else if (!window.isSecureContext || !navigator.mediaDevices?.getUserMedia || !window.AudioWorkletNode) {
  show("Safari benötigt die HTTPS-Verbindung mit vertrauenswürdigem WebMic-Zertifikat.", "error");
  toggle.disabled = true;
} else {
  toggle.addEventListener("click", () => current ? stop(current) : void start());
  window.addEventListener("pagehide", () => {
    if (current) stop(current);
  });
}
