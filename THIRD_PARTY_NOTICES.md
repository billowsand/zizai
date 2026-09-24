# Third-party notices

## auto-voice

Parts of the local speech-recognition, microphone-capture, and audio-resampling implementation are derived from auto-voice.

MIT License

Copyright (c) 2026 billowsand

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## bundled 模型（如装的就是这一版安装）

bundled 安装包在 `data\voice\sense-voice`、`data\voice\punctuation`、`data\voice\hr`
下随包携带下列第三方资源。这些是 sherpa-onnx 项目（k2-fsa）发布的预转换模型与词典，
随包仅用于本地语音输入，不联网、不外发。许可原文见 https://github.com/k2-fsa/sherpa-onnx/blob/master/LICENSE 。

### sherpa-onnx v1.13.8 与 SenseVoice / 标点恢复 / 同音词替换资源

Copyright (c) 2024  the respective contributors of k2-fsa/sherpa-onnx.

Licensed under the Apache License, Version 2.0 (the "License"); you may not use
these files except in compliance with the License. You may obtain a copy of the
License at:

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software distributed
under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
CONDITIONS OF ANY KIND, either express or implied. See the License for the
specific language governing permissions and limitations under the License.

随包资源：

- SenseVoice：`data\voice\sense-voice\model.int8.onnx` 与 `tokens.txt`，源自
  `sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17`（上游仓库
  k2-fsa/sherpa-onnx release `asr-models`）。
- 标点恢复：`data\voice\punctuation\model.int8.onnx`，源自
  `sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8`（上游
  k2-fsa/sherpa-onnx release `punctuation-models`）。
- 同音词替换：`data\voice\hr\lexicon.txt` 与 `data\voice\hr\replace.fst`，源自
  k2-fsa/sherpa-onnx 仓库 `scripts/homophone_replacer` 与 `homophone_replacer/resources`。
