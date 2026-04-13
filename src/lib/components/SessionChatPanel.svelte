<script lang="ts">
  import { Button } from 'bits-ui';
  import { afterUpdate, onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi';
  import Icon from './ui/Icon.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import { autoConfigureSttFast, connectChat, disconnectChat, getBehaviorSettings, getSttConfig, loadStatus, setBehaviorSettings, setModel, setRecordingSpeechBlock, setVoiceEnabled, stopBotSpeech, submitStreamerPrompt, transcribeMicChunk } from '../api/tauri';
  import { authSessionsStore, botLogStore, chatStore, diagnosticsStore, errorBannerStore, eventStore, statusStore } from '../stores/app';
  import { cohostControlsStore, type CohostModelMode } from '../stores/cohost';
  import { chooseBestUtterance, isNonSpeechCaption, mergeTranscriptText, normalizeSpeechText, recordUtteranceCandidate, seemsWeakTranscript, type UtteranceCandidate } from '../voice/utterance';

  let content = '';
  let sttReady = false;
  let sttTimer: number | null = null;
  let sendBlinkTimer: number | null = null;
  let micLive = false;
  let micProcessing = false;
  let micStatus = 'Mic idle.';
  let micChunkMs = 2200;
  let micLoopId = 0;
  let lastMicTextNormalized = '';
  let lastMicTextAt = 0;
  let sttStatusNote = 'STT not initialized.';
  let sttFixing = false;
  let lastAppliedModelMode: CohostModelMode | null = null;
  let lastAppliedPaceSignature = '';
  let selectedModelMode: CohostModelMode = 'medium';
  let videoRemarksPerMinute = 0.6;
  let autonomousReplies = false;
  let postBotRepliesToTwitch = false;
  let topicContinuationMode = false;
  let controlsReady = false;
  let feedEl: HTMLDivElement | null = null;
  let avatarChannel: BroadcastChannel | null = null;
  let liveTranscriptDraft = '';
  let micBuffer = '';
  let micCandidates: UtteranceCandidate[] = [];
  let lastTranscriptChunkNormalized = '';
  let micBufferStartedAt = 0;
  let lastBufferChangedAt = 0;
  let repeatedChunkCount = 0;
  let micSpeechGateActive = false;
  let waitingForReply = false;
  let waitingForReplySince = 0;
  let sendBlink = false;
  let twitchModeBusy = false;
  let conversationPhase: 'idle' | 'ready' | 'listening' | 'processing' | 'replying' = 'idle';
  let controlStatus = 'Mode Medium. Auto comments off. Toggle mic on to listen.';
  const AVATAR_WINDOW_STATE_KEY = 'cohost_avatar_window_state_v1';

  function scheduledMinutesForPace(rate: number): number | null {
    if (rate <= 0) return null;
    if (rate >= 3.5) return 1;
    if (rate >= 2.0) return 2;
    if (rate >= 1.0) return 3;
    if (rate >= 0.6) return 5;
    if (rate >= 0.3) return 10;
    return 15;
  }

  function replyIntervalMsForPace(rate: number): number {
    if (rate <= 0) return 60_000;
    return Math.max(1200, Math.min(60_000, Math.round(60_000 / rate)));
  }

  function modelForMode(mode: CohostModelMode): string {
    if (mode === 'fast') return 'llama3.2:3b';
    if (mode === 'long_context') return 'gemma3:12b';
    return 'qwen3:8b';
  }

  const modeMeta: Record<CohostModelMode, { label: string; detail: string }> = {
    fast: {
      label: 'Fast',
      detail: 'Smallest model. Fastest back-and-forth.'
    },
    medium: {
      label: 'Medium',
      detail: 'Balanced speed and context.'
    },
    long_context: {
      label: 'Long',
      detail: 'Slowest, but best context retention.'
    }
  };
  const modeOrder: CohostModelMode[] = ['fast', 'medium', 'long_context'];

  $: combined = [
    ...$chatStore.map((m) => ({ ...m, source: 'viewer' as const })),
    ...$botLogStore.map((m) => ({ ...m, source: 'bot' as const })),
    ...$eventStore.map((m) => ({
      id: `event-${m.id}`,
      user: 'system',
      content: `${m.kind}: ${m.content}`,
      timestamp: m.timestamp,
      source: 'system' as const
    }))
  ]
    .sort((a, b) => Date.parse(a.timestamp) - Date.parse(b.timestamp))
    .slice(-300);

  afterUpdate(() => {
    if (!feedEl) return;
    requestAnimationFrame(() => {
      if (!feedEl) return;
      feedEl.scrollTop = feedEl.scrollHeight;
    });
  });

  onMount(() => {
    const initialControls = get(cohostControlsStore);
    selectedModelMode = initialControls.modelMode;
    videoRemarksPerMinute = initialControls.videoRemarksPerMinute;
    autonomousReplies = initialControls.autonomousReplies;
    if (typeof BroadcastChannel !== 'undefined') {
      avatarChannel = new BroadcastChannel('cohost-avatar-events');
      avatarChannel.onmessage = (event) => {
        const msg = event.data || {};
        if (msg.type === 'snap_window' && msg.size) {
          const width = Math.max(420, Math.min(1600, Number(msg.size.width || 584)));
          const height = Math.max(520, Math.min(1600, Number(msg.size.height || 820)));
          void (async () => {
            try {
              const existing = await WebviewWindow.getByLabel('cohost-avatar');
              if (!existing) return;
              await existing.setSize(new LogicalSize(width, height));
              await existing.setFocus();
            } catch {
              // no-op
            }
          })();
        }
      };
    }
    controlsReady = true;
    void refreshSttReady();
    void hydrateBehaviorSettings();
    sttTimer = window.setInterval(() => void refreshSttReady(), 3500);
  });

  onDestroy(() => {
    void cleanupRecorder();
    if (sttTimer !== null) {
      window.clearInterval(sttTimer);
    }
    if (sendBlinkTimer !== null) {
      window.clearTimeout(sendBlinkTimer);
    }
    if (avatarChannel) {
      avatarChannel.close();
      avatarChannel = null;
    }
  });

  async function refreshSttReady() {
    try {
      const cfg = await getSttConfig();
      sttReady = !!(cfg.sttBinaryPath && cfg.sttModelPath && cfg.sttEnabled);
      sttStatusNote = sttReady
        ? 'STT ready.'
        : `STT not ready (${cfg.sttEnabled ? 'missing binary/model' : 'disabled'}).`;
    } catch {
      sttReady = false;
      sttStatusNote = 'STT status unavailable.';
    }
  }

  async function hydrateBehaviorSettings() {
    try {
      const currentControls = get(cohostControlsStore);
      selectedModelMode = currentControls.modelMode;
      videoRemarksPerMinute = currentControls.videoRemarksPerMinute;
      autonomousReplies = currentControls.autonomousReplies;
      const behavior = await getBehaviorSettings();
      postBotRepliesToTwitch = !!behavior.postBotMessagesToTwitch;
      topicContinuationMode = !!behavior.topicContinuationMode;
      const desiredMinutes = currentControls.autonomousReplies ? scheduledMinutesForPace(currentControls.videoRemarksPerMinute) : null;
      const desiredInterval = replyIntervalMsForPace(currentControls.videoRemarksPerMinute);
      if (
        behavior.cohostMode !== currentControls.autonomousReplies ||
        (behavior.scheduledMessagesMinutes ?? null) !== desiredMinutes ||
        (behavior.minimumReplyIntervalMs ?? 0) !== desiredInterval
      ) {
        await setBehaviorSettings(
          currentControls.autonomousReplies,
          desiredMinutes,
          desiredInterval,
          postBotRepliesToTwitch,
          topicContinuationMode
        );
      }
    } catch {
      // no-op
    }
  }

  function setConversationPhase(next: 'idle' | 'ready' | 'listening' | 'processing' | 'replying') {
    if (conversationPhase === next) return;
    conversationPhase = next;
  }

  function triggerSendBlink() {
    sendBlink = true;
    if (sendBlinkTimer !== null) window.clearTimeout(sendBlinkTimer);
    sendBlinkTimer = window.setTimeout(() => {
      sendBlink = false;
      sendBlinkTimer = null;
    }, 520);
  }

  async function ensureSttReady(): Promise<boolean> {
    await refreshSttReady();
    if (sttReady) return true;
    if (sttFixing) return false;
    sttFixing = true;
    try {
      const result = await autoConfigureSttFast();
      sttStatusNote = result.message || sttStatusNote;
    } catch (error) {
      sttStatusNote = `STT auto-fix failed: ${String(error)}`;
    } finally {
      sttFixing = false;
    }
    await refreshSttReady();
    return sttReady;
  }

  async function submit() {
    if (!content.trim()) return;
    const outgoing = content.trim();
    content = '';
    try {
      await submitStreamerPrompt(outgoing);
    } catch (error) {
      errorBannerStore.set('Local AI send failed: ' + String(error));
    }
  }

  async function applyModelMode(mode: CohostModelMode) {
    if (mode === lastAppliedModelMode) return;
    lastAppliedModelMode = mode;
    try {
      await setModel(modelForMode(mode));
      await loadStatus();
    } catch (error) {
      errorBannerStore.set('Model mode switch failed: ' + String(error));
    }
  }

  async function toggleAutonomousReplies(enabled: boolean) {
    try {
      await setBehaviorSettings(
        enabled,
        enabled ? scheduledMinutesForPace(videoRemarksPerMinute) : null,
        replyIntervalMsForPace(videoRemarksPerMinute),
        postBotRepliesToTwitch,
        topicContinuationMode
      );
    } catch (error) {
      errorBannerStore.set('Autonomous chatter update failed: ' + String(error));
    }
  }

  async function setBotTwitchPosting(next: boolean) {
    postBotRepliesToTwitch = next;
    controlStatus = `Bot posting to Twitch ${next ? 'enabled' : 'disabled'}.`;
    try {
      await setBehaviorSettings(
        autonomousReplies,
        autonomousReplies ? scheduledMinutesForPace(videoRemarksPerMinute) : null,
        replyIntervalMsForPace(videoRemarksPerMinute),
        next,
        topicContinuationMode
      );
    } catch (error) {
      errorBannerStore.set('Bot Twitch posting update failed: ' + String(error));
    }
  }

  function setMode(mode: CohostModelMode) {
    selectedModelMode = mode;
    controlStatus = `Reply mode set to ${modeMeta[mode].label}.`;
  }

  function setAutonomousReplies(next: boolean) {
    autonomousReplies = next;
    controlStatus = `Auto comments ${next ? 'enabled' : 'disabled'}.`;
    void toggleAutonomousReplies(next);
  }

  async function setTopicContinuationMode(next: boolean) {
    topicContinuationMode = next;
    controlStatus = `Keep talking mode ${next ? 'enabled' : 'disabled'}.`;
    try {
      await setBehaviorSettings(
        autonomousReplies,
        autonomousReplies ? scheduledMinutesForPace(videoRemarksPerMinute) : null,
        replyIntervalMsForPace(videoRemarksPerMinute),
        postBotRepliesToTwitch,
        next
      );
    } catch (error) {
      errorBannerStore.set('Keep talking update failed: ' + String(error));
    }
  }

  $: if (controlsReady) {
    cohostControlsStore.set({
      modelMode: selectedModelMode,
      videoRemarksPerMinute,
      autonomousReplies
    });
  }

  $: if (controlsReady) {
    const signature = [
      autonomousReplies ? '1' : '0',
      postBotRepliesToTwitch ? '1' : '0',
      topicContinuationMode ? '1' : '0',
      String(autonomousReplies ? scheduledMinutesForPace(videoRemarksPerMinute) ?? 0 : 0),
      String(replyIntervalMsForPace(videoRemarksPerMinute))
    ].join(':');
    if (signature !== lastAppliedPaceSignature) {
      lastAppliedPaceSignature = signature;
      void setBehaviorSettings(
        autonomousReplies,
        autonomousReplies ? scheduledMinutesForPace(videoRemarksPerMinute) : null,
        replyIntervalMsForPace(videoRemarksPerMinute),
        postBotRepliesToTwitch,
        topicContinuationMode
      );
    }
  }

  $: void applyModelMode(selectedModelMode);

  async function cleanupRecorder() {
    micLive = false;
    micLoopId += 1;
    micBuffer = '';
    micCandidates = [];
    lastTranscriptChunkNormalized = '';
    micBufferStartedAt = 0;
    lastBufferChangedAt = 0;
    repeatedChunkCount = 0;
    micSpeechGateActive = false;
    waitingForReply = false;
    waitingForReplySince = 0;
    liveTranscriptDraft = '';
    micProcessing = false;
  }

  async function startLiveMic() {
    await setVoiceEnabled(true);
    stopBotSpeech();
    setRecordingSpeechBlock(false);
    micBuffer = '';
    micCandidates = [];
    lastTranscriptChunkNormalized = '';
    micBufferStartedAt = 0;
    lastBufferChangedAt = 0;
    repeatedChunkCount = 0;
    micSpeechGateActive = false;
    waitingForReply = false;
    waitingForReplySince = 0;
    liveTranscriptDraft = '';
    micLive = true;
    setConversationPhase('ready');
    micStatus = 'Mic live. Listening for speech.';
    const thisLoop = ++micLoopId;

    const submitBufferedSpeech = async () => {
      const transcribed = chooseBestUtterance(micBuffer, micCandidates).trim();
      liveTranscriptDraft = transcribed;
      micBuffer = '';
      micCandidates = [];
      lastTranscriptChunkNormalized = '';
      micBufferStartedAt = 0;
      lastBufferChangedAt = 0;
      repeatedChunkCount = 0;
      if (micSpeechGateActive) {
        setRecordingSpeechBlock(false);
        micSpeechGateActive = false;
      }
      if (!transcribed || isNonSpeechCaption(transcribed)) {
        micStatus = 'Listening…';
        return;
      }
      const transcriptWords = normalizeSpeechText(transcribed).split(' ').filter(Boolean).length;
      if (seemsWeakTranscript(transcribed) && transcriptWords <= 1) {
        micStatus = `Weak transcript: "${transcribed}"`;
        return;
      }
      const normalized = normalizeSpeechText(transcribed);
      const now = Date.now();
      if (normalized && normalized === lastMicTextNormalized && now - lastMicTextAt < 7000) {
        micStatus = 'Skipped duplicate phrase.';
        return;
      }
      lastMicTextNormalized = normalized;
      lastMicTextAt = now;
      waitingForReply = true;
      waitingForReplySince = now;
      setConversationPhase('processing');
      triggerSendBlink();
      try {
        await submitStreamerPrompt(transcribed);
        micStatus = `Heard: "${transcribed}"`;
      } catch (error) {
        waitingForReply = false;
        waitingForReplySince = 0;
        micStatus = `Send failed: ${String(error)}`;
        errorBannerStore.set('Local AI send failed: ' + String(error));
      }
    };

    void (async () => {
      let consecutiveErrors = 0;
      while (micLive && thisLoop === micLoopId) {
        try {
          const runtime = window as unknown as {
            __cohost_tts_speaking?: boolean;
            __cohost_last_bot_reply_at?: number;
          };
          if (runtime.__cohost_tts_speaking) {
            if (micSpeechGateActive) {
              setRecordingSpeechBlock(false);
              micSpeechGateActive = false;
            }
            micBuffer = '';
            micCandidates = [];
            lastTranscriptChunkNormalized = '';
            micBufferStartedAt = 0;
            lastBufferChangedAt = 0;
            repeatedChunkCount = 0;
            liveTranscriptDraft = '';
            setConversationPhase('replying');
            micStatus = 'Paused while bot speaks.';
            await new Promise((resolve) => setTimeout(resolve, 180));
            continue;
          }
          if (waitingForReply) {
            const lastBotReplyAt = typeof runtime.__cohost_last_bot_reply_at === 'number' ? runtime.__cohost_last_bot_reply_at : 0;
            const recentReply = lastBotReplyAt >= waitingForReplySince - 250;
            if (recentReply && Date.now() - lastBotReplyAt < 900) {
              if (micSpeechGateActive) {
                setRecordingSpeechBlock(false);
                micSpeechGateActive = false;
              }
              setConversationPhase('replying');
              micStatus = 'Bot replying...';
              await new Promise((resolve) => setTimeout(resolve, 180));
              continue;
            }
            if (!recentReply && Date.now() - waitingForReplySince < 1800) {
              if (micSpeechGateActive) {
                setRecordingSpeechBlock(false);
                micSpeechGateActive = false;
              }
              setConversationPhase('processing');
              micStatus = 'Thinking...';
              await new Promise((resolve) => setTimeout(resolve, 180));
              continue;
            }
            if (!recentReply && Date.now() - waitingForReplySince >= 8000) {
              waitingForReply = false;
              waitingForReplySince = 0;
              micStatus = 'Reply timed out. Listening again.';
              setConversationPhase('ready');
            }
            waitingForReply = false;
            waitingForReplySince = 0;
            setConversationPhase('ready');
            micStatus = 'Mic live. Listening for speech.';
          }

          micProcessing = true;
          const text = (await transcribeMicChunk(micChunkMs)).trim();
          if (!micLive || thisLoop !== micLoopId) {
            break;
          }
          consecutiveErrors = 0;
          if (!text || isNonSpeechCaption(text)) {
            if (micBuffer && lastBufferChangedAt > 0 && Date.now() - lastBufferChangedAt >= 1600) {
              await submitBufferedSpeech();
            } else if (micBuffer && micBufferStartedAt > 0 && Date.now() - micBufferStartedAt >= 7000) {
              await submitBufferedSpeech();
            } else {
              if (!micBuffer && micSpeechGateActive) {
                setRecordingSpeechBlock(false);
                micSpeechGateActive = false;
              }
              setConversationPhase(micBuffer ? 'listening' : 'ready');
              micStatus = micBuffer ? `Listening… ${micBuffer}` : 'Mic live. Listening for speech.';
            }
          } else {
            if (!micSpeechGateActive) {
              stopBotSpeech();
              setRecordingSpeechBlock(true);
              micSpeechGateActive = true;
            }
            const normalizedChunk = normalizeSpeechText(text);
            repeatedChunkCount = normalizedChunk && normalizedChunk === lastTranscriptChunkNormalized
              ? repeatedChunkCount + 1
              : 1;
            if (normalizedChunk && normalizedChunk !== lastTranscriptChunkNormalized) {
              if (!micBufferStartedAt) micBufferStartedAt = Date.now();
              lastTranscriptChunkNormalized = normalizedChunk;
              micBuffer = mergeTranscriptText(micBuffer, text);
              micCandidates = recordUtteranceCandidate(micCandidates, text);
              micCandidates = recordUtteranceCandidate(micCandidates, micBuffer);
              liveTranscriptDraft = micBuffer;
              lastBufferChangedAt = Date.now();
              setConversationPhase('listening');
              micStatus = `Listening… ${micBuffer}`;
              if (Date.now() - micBufferStartedAt >= 7000) {
                await submitBufferedSpeech();
              }
            } else if (micBuffer && repeatedChunkCount >= 2) {
              await submitBufferedSpeech();
            }
          }
        } catch (error) {
          consecutiveErrors += 1;
          const msg = String(error);
          micStatus = `Recording error: ${msg}`;
          if (consecutiveErrors >= 3) {
            errorBannerStore.set('Mic transcription failed: ' + msg);
            break;
          }
        } finally {
          micProcessing = false;
        }
        await new Promise((resolve) => setTimeout(resolve, 40));
      }
      micProcessing = false;
      if (thisLoop === micLoopId && consecutiveErrors >= 3) {
        micLive = false;
        setRecordingSpeechBlock(false);
        setConversationPhase('idle');
        micStatus = 'Mic stopped due to repeated STT errors.';
      }
    })();
  }

  async function stopLiveMic() {
    if (!micLive) return;
    micLive = false;
    micLoopId += 1;
    try {
      micBuffer = '';
      micCandidates = [];
      lastTranscriptChunkNormalized = '';
      micBufferStartedAt = 0;
      lastBufferChangedAt = 0;
      repeatedChunkCount = 0;
      micSpeechGateActive = false;
      waitingForReply = false;
      waitingForReplySince = 0;
      liveTranscriptDraft = '';
    } finally {
      micProcessing = false;
      setRecordingSpeechBlock(false);
      setConversationPhase('idle');
      micStatus = 'Mic off.';
    }
  }

  function loadAvatarWindowState(): { width?: number; height?: number; x?: number; y?: number } | null {
    try {
      const raw = localStorage.getItem(AVATAR_WINDOW_STATE_KEY);
      if (!raw) return null;
      return JSON.parse(raw) as { width?: number; height?: number; x?: number; y?: number };
    } catch {
      return null;
    }
  }

  function saveAvatarWindowState(next: { width?: number; height?: number; x?: number; y?: number }) {
    try {
      const current = loadAvatarWindowState() || {};
      localStorage.setItem(AVATAR_WINDOW_STATE_KEY, JSON.stringify({ ...current, ...next }));
    } catch {
      // no-op
    }
  }

  async function attachAvatarWindowPersistence(win: WebviewWindow) {
    const tagged = win as WebviewWindow & { __cohostPersistenceHooked?: boolean };
    if (tagged.__cohostPersistenceHooked) return;
    tagged.__cohostPersistenceHooked = true;

    await win.onResized(({ payload }) => {
      saveAvatarWindowState({
        width: Number(payload.width),
        height: Number(payload.height)
      });
    });

    await win.onMoved(({ payload }) => {
      saveAvatarWindowState({
        x: Number(payload.x),
        y: Number(payload.y)
      });
    });
  }

  async function toggleMicInline() {
    if (micLive) {
      await stopLiveMic();
      return;
    }
    if (micProcessing) {
      return;
    }
    micStatus = 'Starting mic...';
    setConversationPhase('processing');
    stopBotSpeech();
    setRecordingSpeechBlock(false);
    if (!(await ensureSttReady())) {
      errorBannerStore.set(`STT is not ready. ${sttStatusNote} Go to Settings -> Voice if needed.`);
      setConversationPhase('idle');
      micStatus = 'Mic unavailable.';
      return;
    }
    try {
      await startLiveMic();
    } catch (error) {
      await cleanupRecorder();
      setConversationPhase('idle');
      micStatus = 'Mic start failed.';
      errorBannerStore.set('Mic start failed: ' + String(error));
    }
  }

  async function toggleTwitchMode() {
    if (twitchModeBusy) return;
    twitchModeBusy = true;
    try {
      if ($statusStore.twitchState === 'connected') {
        await disconnectChat();
        await loadStatus();
        controlStatus = 'Switched to local-only mode.';
        return;
      }
      if (!$authSessionsStore.botTokenPresent || !$authSessionsStore.streamerTokenPresent) {
        errorBannerStore.set('Connect bot and streamer first to enable Twitch mode.');
        return;
      }
      await connectChat();
      await loadStatus();
      controlStatus = 'Switched to Twitch live mode.';
    } catch (error) {
      errorBannerStore.set('Twitch mode switch failed: ' + String(error));
    } finally {
      twitchModeBusy = false;
    }
  }

  async function openAvatarQuick() {
    const savedImage = (localStorage.getItem('cohost_avatar_image') || '').trim();
    if (!savedImage) {
      localStorage.setItem('cohost_avatar_image', '/floating-head.png');
    }
    try {
      const existing = await WebviewWindow.getByLabel('cohost-avatar');
      if (existing) {
        await attachAvatarWindowPersistence(existing);
        const visible = await existing.isVisible();
        if (visible) {
          await existing.hide();
          return;
        }
        try {
          const parsed = loadAvatarWindowState();
          if (parsed) {
            const w = Math.max(320, Math.min(1200, Number(parsed.width || 584)));
            const h = Math.max(420, Math.min(1500, Number(parsed.height || 820)));
            await existing.setSize(new LogicalSize(w, h));
            if (typeof parsed.x === 'number' && typeof parsed.y === 'number') {
              await existing.setPosition(new LogicalPosition(parsed.x, parsed.y));
            }
          }
        } catch {
          // no-op
        }
        await existing.show();
        await existing.setFocus();
        return;
      }
      let popupW = 584;
      let popupH = 820;
      let popupX: number | null = null;
      let popupY: number | null = null;
      try {
        const parsed = loadAvatarWindowState();
        if (parsed) {
          popupW = Math.max(320, Math.min(1200, Number(parsed.width || 584)));
          popupH = Math.max(420, Math.min(1500, Number(parsed.height || 820)));
          popupX = typeof parsed.x === 'number' ? parsed.x : null;
          popupY = typeof parsed.y === 'number' ? parsed.y : null;
        }
      } catch {
        // no-op
      }
      const win = new WebviewWindow('cohost-avatar', {
        url: '/avatar-popup.html',
        title: 'Cohost Avatar',
        width: popupW,
        height: popupH,
        x: popupX ?? undefined,
        y: popupY ?? undefined,
        minWidth: 420,
        minHeight: 520,
        resizable: true,
        alwaysOnTop: true,
        transparent: false,
        backgroundColor: '#000000'
      });
      void attachAvatarWindowPersistence(win);
      win.once('tauri://error', (e) => {
        errorBannerStore.set(`Failed to open avatar window: ${String((e as { payload?: unknown })?.payload ?? 'unknown error')}`);
      });
    } catch (error) {
      errorBannerStore.set('Avatar launch failed: ' + String(error));
    }
  }
</script>

<section class="card grid session-chat-panel">
  <div class="head">
    <h3>Main Session Chat Control</h3>
    <div class="health">
      <span class="chip {$authSessionsStore.botTokenPresent ? 'ok' : 'bad'}">Bot {$authSessionsStore.botTokenPresent ? 'ready' : 'missing'}</span>
      <span class="chip {$authSessionsStore.streamerTokenPresent ? 'ok' : 'bad'}">Streamer {$authSessionsStore.streamerTokenPresent ? 'ready' : 'missing'}</span>
      <span class="chip {$statusStore.twitchState === 'connected' ? 'ok' : 'bad'}">Chat {$statusStore.twitchState === 'connected' ? 'joined' : 'not joined'}</span>
      <span class="chip {$diagnosticsStore.providerState === 'connected' ? 'ok' : 'bad'}">AI {$diagnosticsStore.providerState === 'connected' ? 'online' : 'offline'}</span>
      <span class="chip {sttReady ? 'ok' : 'bad'}">STT {sttReady ? 'ready' : 'missing'}</span>
      <span class="chip {micLive ? 'ok' : 'bad'}">Mic {micLive ? 'live' : 'off'}</span>
      <div class="session-toggle-group">
        <button
          type="button"
          class="session-mode-toggle inline {$statusStore.twitchState === 'connected' ? 'online' : 'offline'}"
          on:click={toggleTwitchMode}
          disabled={twitchModeBusy}
          aria-busy={twitchModeBusy}
          title={$statusStore.twitchState === 'connected' ? 'Switch to local-only mode' : 'Join Twitch chat'}
        >
          <span class="mode-light" aria-hidden="true"></span>
          <span class="mode-copy">
            <strong>{$statusStore.twitchState === 'connected' ? 'Twitch Online' : 'Local Only'}</strong>
          </span>
        </button>
        <button
          type="button"
          class="session-mode-toggle inline small {postBotRepliesToTwitch ? 'online' : 'offline'}"
          on:click={() => setBotTwitchPosting(!postBotRepliesToTwitch)}
          title={postBotRepliesToTwitch ? 'Bot replies can post to Twitch chat' : 'Bot replies stay local in the app'}
        >
          <span class="mode-light" aria-hidden="true"></span>
          <span class="mode-copy">
            <strong>{postBotRepliesToTwitch ? 'Bot Twitch' : 'Bot Local'}</strong>
          </span>
        </button>
      </div>
    </div>
  </div>

  <div class="session-main-grid">
    <div class="session-feed-column">
      <div class="feed" bind:this={feedEl}>
        {#if combined.length === 0}
          <small class="muted">No chat or bot activity yet.</small>
        {:else}
          {#each combined as line (line.id)}
            <div class="line {line.source}">
              <span class="tag">
                {line.source === 'bot' ? 'Bot' : line.source === 'system' ? 'System' : 'Chat'}
              </span>
              <strong>{line.user}</strong>
              <span>{line.content}</span>
            </div>
          {/each}
        {/if}
      </div>

      <div class="composer">
        <input bind:value={content} placeholder="Send local message to AI (not Twitch chat)..." on:keydown={(e) => e.key === 'Enter' && submit()} />
        <Button.Root class="p-btn btn" on:click={submit}><Icon name="send" />Send to AI</Button.Root>
        <button
          type="button"
          class="btn composer-action-btn mic-icon {micLive ? 'live' : 'off'}"
          on:click={toggleMicInline}
          disabled={micProcessing}
          aria-busy={micProcessing}
          title={micLive ? 'Turn mic off' : 'Turn mic on'}
          aria-label={micLive ? 'Turn mic off' : 'Turn mic on'}
        >
          <Icon name="mic" />
          <span class="label">{micLive ? 'Mic Off' : 'Mic On'}</span>
        </button>
        <button
          type="button"
          class="btn composer-action-btn avatar-icon"
          on:click={openAvatarQuick}
          title="Toggle avatar popup"
          aria-label="Toggle avatar popup"
        >
          <Icon name="avatar" />
          <span class="label">Avatar</span>
        </button>
      </div>
    </div>

    <aside class="cohost-controls-panel">
      <div class="cohost-runtime-card">
        <small class="muted session-meta">
          <span class="light {$statusStore.twitchState === 'connected' ? 'on' : 'off'}" aria-hidden="true"></span>
          State: {$statusStore.twitchState} | Channel: {$statusStore.channel || 'not set'} | {micStatus}{sttFixing ? ' Auto-fixing STT…' : ''}
        </small>
        <small class="muted session-meta">{controlStatus} Pace {videoRemarksPerMinute.toFixed(1)}/min.</small>
        <small class="muted session-meta">Conversation phase: {conversationPhase === 'idle' ? 'idle' : conversationPhase === 'ready' ? 'ready for you' : conversationPhase === 'listening' ? 'capturing' : conversationPhase === 'processing' ? 'thinking' : 'replying'}.</small>
        <div class="transcript-status {conversationPhase === 'listening' ? 'recording' : ''} {sendBlink ? 'sent' : ''}">
          <span class="transcript-light" aria-hidden="true"></span>
          <small class="muted session-meta">
            Live transcript: {liveTranscriptDraft || (conversationPhase === 'processing' ? 'Speech captured. Sending to AI...' : 'Waiting for speech...')}
          </small>
        </div>
      </div>
      <div class="cohost-controls-table">
        <div class="cohost-control-row">
          <div class="cohost-control-label">
            <strong>Reply mode</strong>
            <small>{modeMeta[selectedModelMode].detail}</small>
          </div>
          <div class="cohost-control-value">
            <div class="mode-button-group">
              {#each modeOrder as mode}
                <Button.Root
                  class="p-btn btn mode-btn {selectedModelMode === mode ? 'active' : 'ghost'}"
                  on:click={() => setMode(mode)}
                >
                  {modeMeta[mode].label}
                </Button.Root>
              {/each}
            </div>
          </div>
        </div>

        <div class="cohost-control-row compact">
          <div class="cohost-control-label">
            <strong>Auto comments</strong>
            <small>{autonomousReplies ? 'On' : 'Off'}</small>
          </div>
          <div class="cohost-control-value">
            <div class="mode-button-group two-way">
              <Button.Root class="p-btn btn mode-btn {autonomousReplies ? 'ghost' : 'active'}" on:click={() => setAutonomousReplies(false)}>
                Off
              </Button.Root>
              <Button.Root class="p-btn btn mode-btn {autonomousReplies ? 'active' : 'ghost'}" on:click={() => setAutonomousReplies(true)}>
                On
              </Button.Root>
            </div>
          </div>
        </div>

        <div class="cohost-control-row compact">
          <div class="cohost-control-label">
            <strong>Keep talking</strong>
            <small>{topicContinuationMode ? 'Stay on the current topic' : 'Normal reply flow'}</small>
          </div>
          <div class="cohost-control-value">
            <div class="mode-button-group two-way">
              <Button.Root class="p-btn btn mode-btn {topicContinuationMode ? 'ghost' : 'active'}" on:click={() => setTopicContinuationMode(false)}>
                Off
              </Button.Root>
              <Button.Root class="p-btn btn mode-btn {topicContinuationMode ? 'active' : 'ghost'}" on:click={() => setTopicContinuationMode(true)}>
                On
              </Button.Root>
            </div>
          </div>
        </div>

        <div class="cohost-control-row">
          <div class="cohost-control-label">
            <strong>Cohost pace</strong>
            <small>{videoRemarksPerMinute.toFixed(1)} / min</small>
          </div>
          <div class="cohost-control-value">
            <div class="pace-control">
              <UiSlider bind:value={videoRemarksPerMinute} min={0} max={4} step={0.1} ariaLabel="Video comment speed" />
            </div>
          </div>
        </div>
      </div>
    </aside>
  </div>
</section>
