# Third-party notices

## Libraries

LingoSoul links Rust crates listed in `Cargo.lock`, under their own licenses. The main ones:

- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx): Apache-2.0
- [ONNX Runtime](https://github.com/microsoft/onnxruntime) (through sherpa-onnx): MIT
- [cpal](https://github.com/RustAudio/cpal): Apache-2.0
- [ureq](https://github.com/algesten/ureq): MIT or Apache-2.0
- [serde](https://github.com/serde-rs/serde), [serde_json](https://github.com/serde-rs/json), [toml](https://github.com/toml-rs/toml): MIT or Apache-2.0
- [pinyin](https://github.com/mozillazg/rust-pinyin): MIT
- [windows](https://github.com/microsoft/windows-rs): MIT or Apache-2.0

## Speech models

The models are **not** included in this repository or its releases. The embedded assistant downloads them when requested in Settings from the sherpa-onnx release page, and they stay under their authors' terms:

- `silero_vad.onnx`: [Silero VAD](https://github.com/snakers4/silero-vad), MIT.
- `sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20`: keyword spotting model published by the [k2-fsa](https://github.com/k2-fsa/sherpa-onnx) project. See its release page for terms.
- `sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09`: converted by k2-fsa from [ASLP-lab/WSYue-ASR](https://huggingface.co/ASLP-lab/WSYue-ASR) (listed as Apache-2.0), a fine-tune of [FunAudioLLM SenseVoiceSmall](https://huggingface.co/FunAudioLLM/SenseVoiceSmall), whose weights are under the [FunASR model license](https://github.com/modelscope/FunASR/blob/main/MODEL_LICENSE).
