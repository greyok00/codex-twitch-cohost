# Quantitative Conversation Notes

Purpose: reference notes for making `twitch-cohost` communicate more like a real person.

## 1. Turn-taking is fast

- Human turn transitions are typically fast.
- Cross-linguistic work reports average turn-transition gaps clustering around roughly `200-250 ms`, with language means ranging from about `7 ms` to `469 ms`.

Sources:
- Stivers et al. 2009 discussion summary: https://communities.springernature.com/posts/taking-turns-during-conversation
- Max Planck record for the work: https://pure.mpg.de/pubman/faces/ViewItemFullPage.jsp?itemId=item_66202_24

Design implication:
- The bot should not wait several seconds before every reply.
- Default reply starts should feel quick, unless the bot is intentionally hesitating or repairing.

## 2. Overlap is normal

- Conversation often contains short overlap.
- A Frontiers summary reports around `40%` of speaker-transitions involve some overlap, though most overlap is short.
- Roughly `70-82%` of transitions are shorter than `500 ms`.

Source:
- https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2015.00731/full

Design implication:
- User barge-in should be treated as normal conversation, not as an error.
- The bot should be interruptible while speaking.

## 3. Repair happens often

- Other-initiated repair occurs about once every `1.4 minutes` across languages.

Sources:
- https://pubmed.ncbi.nlm.nih.gov/26375483/
- https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0136100

Design implication:
- The bot needs short repair moves:
  - `huh?`
  - `wait, say that again`
  - `I only caught half of that`
  - `you cut out`

## 4. Filled pauses are common but sparse

- Spoken-corpus work on BNC2014 reports tens of thousands of filled-pause tokens in a multi-million-word spoken dataset.
- Combined filled-pause rate is roughly around `1%` of tokens in that reported subset, not every line.

Sources:
- Corpus paper context: https://pmc.ncbi.nlm.nih.gov/articles/PMC9014665/
- Spoken BNC2014 background: https://corpora.lancs.ac.uk/bnc2014/

Design implication:
- Interjections like `hmm`, `uh`, `wait`, `yeah`, `ugh` should be occasional.
- They should not appear in every reply.

## 5. Turn sizes vary

- Switchboard-based analysis reports mean turn duration around `6.12 s` in one subset, with speech around `3.22 words/s`.

Source:
- https://pmc.ncbi.nlm.nih.gov/articles/PMC8216537/

Design implication:
- Replies should vary in size:
  - some ultra-short
  - many one-sentence
  - some two-sentence
  - longer only when context supports it

## 6. Backchannels matter

- Backchannel rates vary by context; one summary reports a range around `1.2` to `3.2` backchannels per minute depending on setting and familiarity.

Source:
- https://www.frontiersin.org/journals/robotics-and-ai/articles/10.3389/frobt.2023.988042/full

Design implication:
- The bot should be allowed to produce cheap listener-style responses:
  - `mm`
  - `yeah`
  - `right`
  - `damn`
  - `no shot`
  - `okay wait`

## Recommended behavior model for the app

This section is an implementation inference from the sources above, not a quoted paper result.

- `15-20%` ultra-short reactions
- `45-55%` short one-sentence replies
- `20-30%` medium replies with two short sentences
- `5-10%` longer replies only when the user or context clearly supports it

Additional implementation rules:
- allow interruption while TTS is speaking
- keep a resumable interrupted-thought buffer
- use repair responses for unclear STT instead of hallucinated full answers
- vary reply size by mode instead of always forcing polished full sentences
- keep fillers and backchannels low-frequency and context-sensitive
