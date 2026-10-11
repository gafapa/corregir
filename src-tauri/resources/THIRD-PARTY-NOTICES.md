# Bundled third-party resources

These resources are distributed unmodified. Exact upstream revisions, URLs, file sizes and SHA-256 digests are recorded in `manifest.json`.

- **Pdfium 157.0.8086.0**, provided by the [Pdfium binaries project](https://github.com/bblanchon/pdfium-binaries), includes the Pdfium BSD license and third-party notices in `pdfium/LICENSE` and `pdfium/licenses/`. The upstream archive SHA-256 was verified against the GitHub release digest before extraction: `1fd8af952832dbb0eb16d9249f68fe09e5f5ebf7c3dd9f6066ea2720cc28487d`.
- **Roboto Regular**, from the [Roboto 2 project](https://github.com/googlefonts/roboto-2), copyright Google and its contributors, is licensed under Apache 2.0. The full license is in `fonts/LICENSE.txt`.
- **Ocrs detection and recognition models**, by Robert Knight, are licensed according to the upstream [Hugging Face model card](https://huggingface.co/robertknight/ocrs) under Creative Commons Attribution-ShareAlike 4.0. Their SHA-256 digests match the upstream LFS metadata at the pinned revision. The original model card and full license are in `models/ocr/MODEL-CARD.md` and `models/ocr/LICENSE.txt`. Credit the author and preserve this notice and the license when redistributing the models; modified models must comply with the same license.

The SHA-256 manifest detects unintended resource changes. It does not replace code signing or advisory review. Only Windows x64 is currently supported and tested.
