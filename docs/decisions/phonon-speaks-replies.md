# Replies are spoken by Gradium's Phonon, on the device

**Status:** Decided. Not built.
**First recorded:** 2026-11-20, in #2075.

Conversation mode speaks the agent's replies with Phonon 31m, Gradium's 31-million-parameter speech model, run on the device's CPU through the `ptts` crate. Its first audio comes within tens of milliseconds, it runs many times faster than real time, and it streams, so speech can start as soon as a reply's first phrase is written. It speaks English only, and has no pronunciation dictionary, so it mispronounces some names and technical words. The runtime is under MIT or Apache-2.0, and the weights and voices under Apache-2.0.

Speaking on the device keeps the sound off the network, as dictating does, and needs no account. Of the small models that can run there, Phonon sounds the most natural in Artificial Analysis's listening arena, level with OpenAI's TTS-1 HD.

## Chosen over

- **The system's own voice, such as macOS's `say`.** It needs nothing downloaded, but it sounds robotic next to a neural model, and each platform's is different.
- **Kokoro-82M and Piper.** Both turn text into phonemes first. Piper does it with espeak-ng, which is GPL-3.0, and Kokoro's front end falls back to it, so neither ships as it is, as `no-strong-copyleft.md` says. Kokoro also makes a whole sentence before any audio, and rates below Phonon in the same arena.
- **Kyutai's Pocket TTS,** which Phonon is distilled from. It speaks seven languages, but it's three times the size and slower, rates below Phonon in English, and some of its voices are only for non-commercial use.
- **A cloud voice, such as ElevenLabs.** It sounds the best, but the reply's text goes to another company, it needs an account and costs by the character, and its first audio waits on the network.
