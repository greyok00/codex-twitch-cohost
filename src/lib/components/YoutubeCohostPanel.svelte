<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Button } from 'bits-ui';
  import UiSelect from './ui/UiSelect.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import Icon from './ui/Icon.svelte';
  import { errorBannerStore, personalityStore } from '../stores/app';
  import { cohostControlsStore } from '../stores/cohost';
  import type { DeveloperSnapshot, SessionState, YoutubeCohostSettings, YoutubeHistoryItem } from '../youtube/types';
  import { parseYouTubeHistory, parseYouTubeInput } from '../youtube/utils';
  import { YoutubeCohostSession } from '../youtube/services/YoutubeCohostSession';
  import { TranscriptSourceService } from '../youtube/sources/TranscriptSourceService';

  export let compact = false;

  let youtubeUrl = '';
  let playerHost: HTMLDivElement | null = null;
  let transcriptFileText = '';
  let historyItems: YoutubeHistoryItem[] = [];
  let selectedHistoryVideo = '';
  let lastLoadedLabel = '';
  let loadingVideo = false;
  let lastLoadedVideoId = '';
  let parsedCurrent: ReturnType<typeof parseYouTubeInput> | null = null;
  let previewVideoId = '';
  let previewThumb = '';
  let currentTitle = 'No video loaded';
  let isPlaying = false;

  let snapshot: SessionState = {
    playbackState: 'idle',
    lastError: null,
    currentTime: 0,
    duration: 0,
    lastRemark: '',
    lastDecisionReason: 'idle',
    nextCommentProbability: 0,
    currentSegmentText: '',
    transcriptMode: 'metadata',
    transcriptStatusMessage: 'No transcript loaded yet.',
    transcriptQuality: 'low',
    transcriptCoverage: 0
  };
  let debug: DeveloperSnapshot | null = null;

  const transcriptSources = new TranscriptSourceService();
  let session: YoutubeCohostSession | null = null;
  let unsubscribe: (() => void) | null = null;

  let settings: YoutubeCohostSettings = {
    remarksPerMinute: 1.2,
    relevanceStrictness: 72,
    humorStyle: 'sarcastic',
    maxRemarkLengthSeconds: 8,
    interruptOnlyAtNaturalBreaks: true,
    captionsDebugOverlay: false,
    autoResumeAfterRemark: true,
    developerMode: false
  };

  const humorOptions = [
    { value: 'dry', label: 'Dry' },
    { value: 'sarcastic', label: 'Sarcastic' },
    { value: 'chaotic', label: 'Chaotic' },
    { value: 'deadpan', label: 'Deadpan' },
    { value: 'absurd', label: 'Absurd' }
  ];
  const remarkLengthOptions = [
    { value: '4', label: '4 seconds' },
    { value: '8', label: '8 seconds' },
    { value: '12', label: '12 seconds' }
  ];
  let maxLengthValue = '8';

  $: settings.maxRemarkLengthSeconds = Number(maxLengthValue) as 4 | 8 | 12;
  $: settings.remarksPerMinute = $cohostControlsStore.autonomousReplies ? $cohostControlsStore.videoRemarksPerMinute : 0;
  $: if (session) session.setSettings(settings);
  $: if (session) session.setPersonalityPrompt($personalityStore.master_prompt_override || 'Be extremely funny but context-anchored.');
  $: if (session) session.setModelMode($cohostControlsStore.modelMode);
  $: parsedCurrent = parseYouTubeInput(lastLoadedLabel || youtubeUrl);
  $: previewVideoId = lastLoadedVideoId || parsedCurrent?.videoId || '';
  $: previewThumb = previewVideoId ? `https://i.ytimg.com/vi/${previewVideoId}/hqdefault.jpg` : '';
  $: currentTitle = session?.player.getVideoMetadata().title || lastLoadedLabel || 'No video loaded';
  $: isPlaying = snapshot.playbackState === 'playing' || snapshot.playbackState === 'evaluating_comment' || snapshot.playbackState === 'speaking_remark';

  onMount(async () => {
    try {
      await ensureSession();
    } catch (error) {
      errorBannerStore.set('YouTube session failed to initialize: ' + String(error));
    }
  });

  onDestroy(() => {
    unsubscribe?.();
    session?.destroy();
  });

  async function ensureSession() {
    if (!playerHost || session) return;
    session = new YoutubeCohostSession(playerHost, settings, {
      onError: (message) => errorBannerStore.set('YouTube mode error: ' + message)
    });
    await session.init();
    const onChange = () => {
      if (!session) return;
      snapshot = session.state.getSnapshot();
      debug = session.state.getDebug();
      const meta = session.player.getVideoMetadata();
      if (meta.videoId) lastLoadedVideoId = meta.videoId;
      if (meta.title) currentTitle = meta.title;
    };
    session.state.addEventListener('change', onChange);
    unsubscribe = () => session?.state.removeEventListener('change', onChange);
    onChange();
  }

  async function loadVideo() {
    if (!youtubeUrl.trim()) return;
    await ensureSession();
    if (!session) return;
    loadingVideo = true;
    try {
      const rawInput = youtubeUrl.trim();
      const parsed = parseYouTubeInput(rawInput);
      const ok = session.loadVideo(rawInput);
      if (!ok) {
        errorBannerStore.set('Invalid YouTube URL or video ID.');
        return;
      }
      lastLoadedLabel = rawInput;
      lastLoadedVideoId = parsed?.videoId || '';
      if (parsed?.videoId) {
        await refreshTranscript(parsed.videoId);
      }
      window.setTimeout(() => void refreshTranscript(parsed?.videoId), 1800);
      window.setTimeout(() => void refreshTranscript(parsed?.videoId), 5200);
      session.play();
    } finally {
      loadingVideo = false;
    }
  }

  async function togglePlayback() {
    if (!session) return;
    if (isPlaying) {
      session.pause();
      return;
    }
    session.play();
  }

  async function refreshTranscript(videoIdOverride?: string) {
    if (!session) return;
    const meta = session.player.getVideoMetadata();
    const videoId = videoIdOverride || meta.videoId || lastLoadedVideoId;
    if (!videoId) return;
    const resolved = await transcriptSources.resolve({
      videoId,
      title: meta.title,
      description: '',
      durationSeconds: Math.max(session.player.getDuration(), 60),
      transcriptFileText
    });
    session.setTranscript(resolved, Math.max(session.player.getDuration(), 60));
  }

  async function onTranscriptUpload(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    transcriptFileText = await file.text();
    await refreshTranscript();
  }

  async function onHistoryUpload(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    const content = await file.text();
    historyItems = parseYouTubeHistory(content);
    selectedHistoryVideo = historyItems[0]?.videoId || '';
  }

  async function loadHistorySelection() {
    if (!selectedHistoryVideo) return;
    youtubeUrl = `https://www.youtube.com/watch?v=${selectedHistoryVideo}`;
    await loadVideo();
  }

  function formatTime(seconds: number): string {
    if (!Number.isFinite(seconds) || seconds <= 0) return '0:00';
    const total = Math.floor(seconds);
    const mins = Math.floor(total / 60);
    const secs = total % 60;
    return `${mins}:${String(secs).padStart(2, '0')}`;
  }
</script>

<section class="card grid yt-card {compact ? 'yt-card-compact' : ''}">
  <div class="row wrap yt-topline">
    <div>
      <h3>{compact ? 'YouTube Audio Co-Host' : 'YouTube Co-Host Mode'}</h3>
      <small class="muted">
        {compact
          ? 'Background playback with a small control surface. Heavy controls stay collapsed.'
          : 'Paste a URL, load the player, and the co-host will pause at natural moments to drop short contextual remarks.'}
      </small>
    </div>
    <div class="yt-mini-badges">
      <span class="chip {isPlaying ? 'ok' : 'warn'}">{isPlaying ? 'playing' : snapshot.playbackState}</span>
      <span class="chip {snapshot.transcriptCoverage >= 0.5 ? 'ok' : 'warn'}">captions {(snapshot.transcriptCoverage * 100).toFixed(0)}%</span>
      <span class="chip {snapshot.nextCommentProbability >= 0.45 ? 'ok' : 'warn'}">comment {(snapshot.nextCommentProbability * 100).toFixed(0)}%</span>
    </div>
  </div>

  <div class="yt-load-row {compact ? 'yt-load-row-compact' : ''}">
    <input type="text" bind:value={youtubeUrl} placeholder="https://www.youtube.com/watch?v=... or playlist URL" />
    <Button.Root class="p-btn btn" on:click={loadVideo} disabled={loadingVideo}>
      <Icon name="play" />{loadingVideo ? 'Loading...' : 'Load'}
    </Button.Root>
    <Button.Root class="p-btn btn ghost" on:click={togglePlayback} disabled={!session}>
      <Icon name={isPlaying ? 'pause' : 'play'} />{isPlaying ? 'Pause' : 'Play'}
    </Button.Root>
    <Button.Root class="p-btn btn ghost" on:click={() => void refreshTranscript()} disabled={!session}>
      <Icon name="check" />Captions
    </Button.Root>
  </div>

  <div class="yt-mini-shell">
    <div class="yt-thumb-card">
      {#if previewThumb}
        <img class="yt-thumb-image" src={previewThumb} alt="YouTube thumbnail preview" />
      {:else}
        <div class="yt-thumb-placeholder">
          <Icon name="play" />
          <span>No video loaded</span>
        </div>
      {/if}
    </div>

    <div class="yt-mini-meta">
      <strong class="yt-mini-title">{currentTitle}</strong>
      <small class="muted">{formatTime(snapshot.currentTime)} / {formatTime(snapshot.duration)} | {snapshot.transcriptStatusMessage}</small>
      <small><strong>Last remark:</strong> {snapshot.lastRemark || 'Waiting for context.'}</small>
      <small><strong>Last decision:</strong> {snapshot.lastDecisionReason}</small>
    </div>
  </div>

  <div class="yt-player {compact ? 'yt-player-hidden' : ''}" bind:this={playerHost}></div>

  <details class="settings-diagnostics yt-advanced" open={!compact}>
    <summary>Co-Host controls</summary>

    <div class="yt-upload-row">
      <label class="upload">
        <span>User transcript/subtitle file</span>
        <input type="file" accept=".srt,.vtt,.txt,.json" on:change={onTranscriptUpload} />
      </label>
      <label class="upload">
        <span>YouTube history JSON</span>
        <input type="file" accept=".json" on:change={onHistoryUpload} />
      </label>
      <UiSelect
        bind:value={selectedHistoryVideo}
        options={historyItems.map((item) => ({ value: item.videoId, label: item.title }))}
        placeholder="Select history video"
      />
      <Button.Root class="p-btn btn" on:click={loadHistorySelection} disabled={!selectedHistoryVideo}>
        <Icon name="check" />Load History Item
      </Button.Root>
    </div>

    <div class="yt-controls-grid yt-controls-grid-compact">
      <div class="muted">remarks/min: {settings.remarksPerMinute.toFixed(1)}</div>
      <UiSlider bind:value={settings.remarksPerMinute} min={0} max={4} step={0.1} ariaLabel="Remarks per minute" />

      <div class="muted">strictness: {settings.relevanceStrictness.toFixed(0)}</div>
      <UiSlider bind:value={settings.relevanceStrictness} min={0} max={100} step={1} ariaLabel="Relevance strictness" />

      <div class="muted">humor</div>
      <UiSelect bind:value={settings.humorStyle} options={humorOptions} placeholder="Humor style" />

      <div class="muted">max length</div>
      <UiSelect bind:value={maxLengthValue} options={remarkLengthOptions} placeholder="Max remark length" />
    </div>

    <div class="yt-toggle-row">
      <label class="toggle-row"><input type="checkbox" bind:checked={settings.interruptOnlyAtNaturalBreaks} /> natural breaks only</label>
      <label class="toggle-row"><input type="checkbox" bind:checked={settings.autoResumeAfterRemark} /> auto resume</label>
      <label class="toggle-row"><input type="checkbox" bind:checked={settings.developerMode} /> developer mode</label>
    </div>

    <div class="yt-live-grid yt-live-grid-compact">
      <article class="card-lite">
        <h4>Live status</h4>
        <small><strong>Segment:</strong> {snapshot.currentSegmentText || 'N/A'}</small>
        <small><strong>Transcript mode:</strong> {snapshot.transcriptMode}</small>
        <small><strong>Transcript quality:</strong> {snapshot.transcriptQuality}</small>
        <small><strong>Coverage:</strong> {(snapshot.transcriptCoverage * 100).toFixed(0)}%</small>
      </article>

      {#if settings.developerMode}
        <article class="card-lite">
          <h4>Developer</h4>
          <small><strong>Topic:</strong> {debug?.transcriptWindow?.topicSummary || 'N/A'}</small>
          <small><strong>Score:</strong> {debug?.commentDecision?.components.total?.toFixed(3) || '0.000'}</small>
          <small><strong>Reason:</strong> {debug?.reason || 'N/A'}</small>
        </article>
      {/if}
    </div>
  </details>
</section>
