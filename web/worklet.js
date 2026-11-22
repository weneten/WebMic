class WebMicRecorder extends AudioWorkletProcessor {
  constructor() {
    super();
    this.offset = 0;
    this.peak = 0;
    this.allocate();
  }

  allocate() {
    this.samples = new DataView(new ArrayBuffer(3840));
  }

  process(inputs) {
    const channel = inputs[0]?.[0];
    if (!channel) return true;

    for (const sample of channel) {
      const value = Math.max(-1, Math.min(1, sample));
      this.peak = Math.max(this.peak, Math.abs(value));
      this.samples.setInt16(this.offset, Math.round(value * 32767), true);
      this.offset += 2;
      if (this.offset === this.samples.byteLength) {
        const pcm = this.samples.buffer;
        this.port.postMessage({ pcm, peak: this.peak }, [pcm]);
        this.offset = 0;
        this.peak = 0;
        this.allocate();
      }
    }

    return true;
  }
}

registerProcessor("webmic-recorder", WebMicRecorder);
