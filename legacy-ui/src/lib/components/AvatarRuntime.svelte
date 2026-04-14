<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { getSavedAvatarImage } from '../api/tauri';

  export let detached = false;
  export let embedded = false;

  const AVATAR_KEY = 'cohost_avatar_image';
  const MOUTH_KEY = 'cohost_mouth_align_v1';
  const BROW_KEY = 'cohost_brow_align_v1';
  const BROW_STYLE_KEY = 'cohost_brow_style_v1';
  const MOUTH_COLOR_KEY = 'cohost_mouth_color_v1';
  const BROW_COLOR_KEY = 'cohost_brow_color_v1';
  const UI_HIDDEN_KEY = 'cohost_avatar_ui_hidden_v1';
  const SIZE_KEY = 'cohost_avatar_size';

  type MouthAlign = { x: number; y: number; w: number; h: number };
  type BrowAlign = { x: number; y: number; w: number; h: number; g: number };
  type BrowStyle = 'straight' | 'arched' | 'angry';

  const defaults = {
    mouth: { x: 50, y: 74, w: 24, h: 10 } satisfies MouthAlign,
    brows: { x: 50, y: 33, w: 12, h: 2.2, g: 24 } satisfies BrowAlign,
    browStyle: 'straight' as BrowStyle,
    mouthColor: '#32090d',
    browColor: '#111217'
  };

  let avatarSrc = '/floating-head.png';
  let uiHidden = embedded ? false : false;
  let speaking = false;
  let mouthAlign: MouthAlign = { ...defaults.mouth };
  let browAlign: BrowAlign = { ...defaults.brows };
  let browStyle: BrowStyle = defaults.browStyle;
  let mouthColor = defaults.mouthColor;
  let browColor = defaults.browColor;
  let wrapEl: HTMLDivElement | null = null;
  let mouthEl: HTMLDivElement | null = null;
  let browLeftEl: HTMLDivElement | null = null;
  let browRightEl: HTMLDivElement | null = null;
  let avatarChannel: BroadcastChannel | null = null;
  let speakTimer: number | null = null;
  let stepTimer: number | null = null;
  let activeViseme: 'rest' | 'a' | 'e' | 'o' | 'm' | 'f' = 'rest';
  let browLift = 0;

  function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, value));
  }

  function readJson<T extends object>(key: string, fallback: T): T {
    try {
      const raw = localStorage.getItem(key);
      if (!raw) return { ...fallback };
      return { ...fallback, ...JSON.parse(raw) };
    } catch {
      return { ...fallback };
    }
  }

  function saveJson(key: string, value: unknown) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      // no-op
    }
  }

  function readMouth(): MouthAlign {
    const next = readJson(MOUTH_KEY, defaults.mouth);
    return {
      x: clamp(Number(next.x), 10, 90),
      y: clamp(Number(next.y), 10, 95),
      w: clamp(Number(next.w), 6, 60),
      h: clamp(Number(next.h), 4, 30)
    };
  }

  function readBrows(): BrowAlign {
    const next = readJson(BROW_KEY, defaults.brows);
    return {
      x: clamp(Number(next.x), 10, 90),
      y: clamp(Number(next.y), 5, 70),
      w: clamp(Number(next.w), 4, 24),
      h: clamp(Number(next.h), 0.6, 6),
      g: clamp(Number(next.g), 6, 50)
    };
  }

  function syncAvatarSource(src: string) {
    avatarSrc = src || '/floating-head.png';
    try {
      localStorage.setItem(AVATAR_KEY, avatarSrc);
    } catch {
      // no-op
    }
  }

  async function hydrateAvatarSource() {
    const stored = (localStorage.getItem(AVATAR_KEY) || '').trim();
    if (stored) {
      avatarSrc = stored;
      return;
    }
    try {
      const saved = await getSavedAvatarImage();
      syncAvatarSource(saved?.dataUrl || '/floating-head.png');
    } catch {
      syncAvatarSource('/floating-head.png');
    }
  }

  function updateMouth(patch: Partial<MouthAlign>) {
    mouthAlign = { ...mouthAlign, ...patch };
    saveJson(MOUTH_KEY, mouthAlign);
    avatarChannel?.postMessage({ type: 'mouth_align', align: mouthAlign });
  }

  function updateBrows(patch: Partial<BrowAlign>) {
    browAlign = { ...browAlign, ...patch };
    saveJson(BROW_KEY, browAlign);
    avatarChannel?.postMessage({ type: 'brow_align', align: browAlign });
  }

  function setBrowStyle(next: string) {
    browStyle = (['straight', 'arched', 'angry'].includes(next) ? next : defaults.browStyle) as BrowStyle;
    localStorage.setItem(BROW_STYLE_KEY, browStyle);
  }

  function setMouthColor(next: string) {
    mouthColor = /^#[0-9a-f]{6}$/i.test(next) ? next : defaults.mouthColor;
    localStorage.setItem(MOUTH_COLOR_KEY, mouthColor);
  }

  function setBrowColor(next: string) {
    browColor = /^#[0-9a-f]{6}$/i.test(next) ? next : defaults.browColor;
    localStorage.setItem(BROW_COLOR_KEY, browColor);
  }

  function resetAll() {
    mouthAlign = { ...defaults.mouth };
    browAlign = { ...defaults.brows };
    browStyle = defaults.browStyle;
    mouthColor = defaults.mouthColor;
    browColor = defaults.browColor;
    saveJson(MOUTH_KEY, mouthAlign);
    saveJson(BROW_KEY, browAlign);
    localStorage.setItem(BROW_STYLE_KEY, browStyle);
    localStorage.setItem(MOUTH_COLOR_KEY, mouthColor);
    localStorage.setItem(BROW_COLOR_KEY, browColor);
    avatarChannel?.postMessage({ type: 'mouth_align', align: mouthAlign });
    avatarChannel?.postMessage({ type: 'brow_align', align: browAlign });
  }

  function snapWindowToImage() {
    const avatar = document.getElementById('shared-avatar-image') as HTMLImageElement | null;
    const naturalWidth = avatar?.naturalWidth || avatar?.width || 560;
    const naturalHeight = avatar?.naturalHeight || avatar?.height || 760;
    const panelWidth = uiHidden ? 0 : 388;
    const targetWidth = Math.max(420, Math.min(screen.availWidth - 24, naturalWidth + panelWidth + 36));
    const targetHeight = Math.max(520, Math.min(screen.availHeight - 24, naturalHeight + 64));
    try {
      localStorage.setItem(SIZE_KEY, JSON.stringify({ width: targetWidth - 24, height: targetHeight - 60 }));
    } catch {
      // no-op
    }
    avatarChannel?.postMessage({ type: 'snap_window', size: { width: targetWidth, height: targetHeight } });
  }

  function mapCharToViseme(ch: string): 'rest' | 'a' | 'e' | 'o' | 'm' | 'f' {
    const c = ch.toLowerCase();
    if ('aei'.includes(c)) return 'e';
    if ('ou'.includes(c)) return 'o';
    if ('mbp'.includes(c)) return 'm';
    if ('fv'.includes(c)) return 'f';
    if ('rnltdkgszhcjxyqw'.includes(c)) return 'a';
    return 'rest';
  }

  function stopSpeaking() {
    speaking = false;
    activeViseme = 'rest';
    browLift = 0;
    if (speakTimer !== null) window.clearTimeout(speakTimer);
    if (stepTimer !== null) window.clearTimeout(stepTimer);
    speakTimer = null;
    stepTimer = null;
  }

  function startSpeaking(text: string) {
    stopSpeaking();
    const chars = (text || '').replace(/\s+/g, ' ').slice(0, 220).split('');
    const sequence = chars.length ? chars.map((ch) => ({ viseme: mapCharToViseme(ch), dur: ch === ' ' ? 48 : 64 })) : [{ viseme: 'a' as const, dur: 90 }];
    let index = 0;
    speaking = true;

    const step = () => {
      if (!speaking) return;
      const frame = sequence[index % sequence.length];
      activeViseme = frame.viseme;
      browLift = 0.5 + Math.abs(Math.sin(Date.now() / 110)) * 0.9;
      index += 1;
      stepTimer = window.setTimeout(step, frame.dur);
    };

    step();
    const total = sequence.reduce((sum, frame) => sum + frame.dur, 0);
    speakTimer = window.setTimeout(stopSpeaking, Math.max(900, Math.min(8000, total + 180)));
  }

  function makeDragHandler(target: HTMLElement | null, kind: 'mouth' | 'brow') {
    if (!target || !wrapEl) return;
    let dragging = false;
    const clampY = kind === 'mouth' ? [10, 95] : [5, 70];

    const toPct = (clientX: number, clientY: number) => {
      const rect = wrapEl?.getBoundingClientRect();
      if (!rect || !rect.width || !rect.height) return null;
      return {
        x: clamp(((clientX - rect.left) / rect.width) * 100, 10, 90),
        y: clamp(((clientY - rect.top) / rect.height) * 100, clampY[0], clampY[1])
      };
    };

    const onDown = (event: PointerEvent) => {
      dragging = true;
      target.setPointerCapture(event.pointerId);
      const pct = toPct(event.clientX, event.clientY);
      if (pct) {
        if (kind === 'mouth') updateMouth(pct);
        else updateBrows(pct);
      }
    };
    const onMove = (event: PointerEvent) => {
      if (!dragging) return;
      const pct = toPct(event.clientX, event.clientY);
      if (pct) {
        if (kind === 'mouth') updateMouth(pct);
        else updateBrows(pct);
      }
    };
    const onUp = (event: PointerEvent) => {
      dragging = false;
      try {
        target.releasePointerCapture(event.pointerId);
      } catch {
        // no-op
      }
    };

    target.addEventListener('pointerdown', onDown);
    target.addEventListener('pointermove', onMove);
    target.addEventListener('pointerup', onUp);
    target.addEventListener('pointercancel', onUp);

    return () => {
      target.removeEventListener('pointerdown', onDown);
      target.removeEventListener('pointermove', onMove);
      target.removeEventListener('pointerup', onUp);
      target.removeEventListener('pointercancel', onUp);
    };
  }

  const dragCleanups: Array<() => void> = [];

  onMount(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === AVATAR_KEY) avatarSrc = (localStorage.getItem(AVATAR_KEY) || '').trim() || '/floating-head.png';
      if (event.key === MOUTH_KEY) mouthAlign = readMouth();
      if (event.key === BROW_KEY) browAlign = readBrows();
      if (event.key === BROW_STYLE_KEY) setBrowStyle(localStorage.getItem(BROW_STYLE_KEY) || defaults.browStyle);
      if (event.key === MOUTH_COLOR_KEY) setMouthColor(localStorage.getItem(MOUTH_COLOR_KEY) || defaults.mouthColor);
      if (event.key === BROW_COLOR_KEY) setBrowColor(localStorage.getItem(BROW_COLOR_KEY) || defaults.browColor);
    };

    void (async () => {
      await hydrateAvatarSource();
      mouthAlign = readMouth();
      browAlign = readBrows();
      setBrowStyle(localStorage.getItem(BROW_STYLE_KEY) || defaults.browStyle);
      setMouthColor(localStorage.getItem(MOUTH_COLOR_KEY) || defaults.mouthColor);
      setBrowColor(localStorage.getItem(BROW_COLOR_KEY) || defaults.browColor);
      uiHidden = embedded ? false : localStorage.getItem(UI_HIDDEN_KEY) === '1';

      if (typeof BroadcastChannel !== 'undefined') {
        avatarChannel = new BroadcastChannel('cohost-avatar-events');
        avatarChannel.onmessage = (event) => {
          const msg = event.data || {};
          if (msg.type === 'speak' || msg.type === 'speak_start') startSpeaking(typeof msg.text === 'string' ? msg.text : '');
          if (msg.type === 'speak_stop') stopSpeaking();
          if (msg.type === 'set_mouth_align' && msg.align) mouthAlign = { ...mouthAlign, ...msg.align };
          if (msg.type === 'set_brow_align' && msg.align) browAlign = { ...browAlign, ...msg.align };
        };
      }

      window.addEventListener('storage', onStorage);

      dragCleanups.push(makeDragHandler(mouthEl, 'mouth') || (() => {}));
      dragCleanups.push(makeDragHandler(browLeftEl, 'brow') || (() => {}));
      dragCleanups.push(makeDragHandler(browRightEl, 'brow') || (() => {}));
    })();

    return () => {
      window.removeEventListener('storage', onStorage);
    };
  });

  onDestroy(() => {
    stopSpeaking();
    avatarChannel?.close();
    avatarChannel = null;
    for (const cleanup of dragCleanups) cleanup();
  });

  $: if (!embedded) {
    localStorage.setItem(UI_HIDDEN_KEY, uiHidden ? '1' : '0');
  }

  $: mouthClass = `mouth viseme-${activeViseme}`;
  $: browsClass = `brows style-${browStyle}`;
  $: mouthStyle = `left:${mouthAlign.x}%;top:${mouthAlign.y}%;width:${mouthAlign.w}%;height:${mouthAlign.h}%;background:${mouthColor};`;
  $: leftBrowStyle = `left:calc(${browAlign.x}% - ${browAlign.g}% / 2);top:calc(${browAlign.y}% - ${browLift}%);width:${browAlign.w}%;height:${browAlign.h}%;background:${browColor};`;
  $: rightBrowStyle = `left:calc(${browAlign.x}% + ${browAlign.g}% / 2);top:calc(${browAlign.y}% - ${browLift}%);width:${browAlign.w}%;height:${browAlign.h}%;background:${browColor};`;
</script>

<section class="avatar-runtime {detached ? 'detached' : 'embedded'} {uiHidden ? 'ui-hidden' : ''}">
  <div class="avatar-stage">
    <div class="avatar-jar-shell {speaking ? 'speaking' : ''}">
      <div class="jar-top-rim"></div>
      <div class="jar-glass">
        <div class="jar-reflection jar-reflection-primary"></div>
        <div class="jar-reflection jar-reflection-secondary"></div>
        <div class="jar-reflection jar-reflection-rim"></div>
        <div class="avatar-wrap" bind:this={wrapEl}>
          <img id="shared-avatar-image" class="avatar-image" src={avatarSrc} alt="Avatar" on:error={() => syncAvatarSource('/floating-head.png')} />
          <div class={browsClass}>
            <div class="brow left" bind:this={browLeftEl} style={leftBrowStyle}></div>
            <div class="brow right" bind:this={browRightEl} style={rightBrowStyle}></div>
          </div>
          <div class={mouthClass} bind:this={mouthEl} style={mouthStyle}></div>
        </div>
        <div class="jar-inner-shadow"></div>
      </div>
      <div class="jar-bottom-rim"></div>
      <div class="jar-floor-shadow"></div>
    </div>
  </div>

  {#if !detached || !uiHidden}
    <div class="avatar-tools">
      <div class="avatar-tools-head">
        <strong>Avatar Rig Controls</strong>
        {#if detached}
          <div class="avatar-tool-actions">
            <button type="button" class="btn" on:click={snapWindowToImage}>Snap</button>
            <button type="button" class="btn" on:click={() => (uiHidden = true)}>Hide UI</button>
          </div>
        {/if}
      </div>

      <div class="avatar-controls-grid">
        <div class="avatar-control-group">
          <small class="avatar-group-title">Mouth</small>
          <label><span>X</span><input type="range" min="10" max="90" step="0.1" bind:value={mouthAlign.x} on:input={() => updateMouth({ x: mouthAlign.x })} /></label>
          <label><span>Y</span><input type="range" min="10" max="95" step="0.1" bind:value={mouthAlign.y} on:input={() => updateMouth({ y: mouthAlign.y })} /></label>
          <label><span>W</span><input type="range" min="6" max="60" step="0.1" bind:value={mouthAlign.w} on:input={() => updateMouth({ w: mouthAlign.w })} /></label>
          <label><span>H</span><input type="range" min="4" max="30" step="0.1" bind:value={mouthAlign.h} on:input={() => updateMouth({ h: mouthAlign.h })} /></label>
          <label><span>Fill</span><input type="color" bind:value={mouthColor} on:input={() => setMouthColor(mouthColor)} /></label>
        </div>

        <div class="avatar-control-group">
          <small class="avatar-group-title">Eyebrows</small>
          <label>
            <span>Style</span>
            <select bind:value={browStyle} on:change={() => setBrowStyle(browStyle)}>
              <option value="straight">Straight</option>
              <option value="arched">Arched</option>
              <option value="angry">Angry</option>
            </select>
          </label>
          <label><span>X</span><input type="range" min="10" max="90" step="0.1" bind:value={browAlign.x} on:input={() => updateBrows({ x: browAlign.x })} /></label>
          <label><span>Y</span><input type="range" min="5" max="70" step="0.1" bind:value={browAlign.y} on:input={() => updateBrows({ y: browAlign.y })} /></label>
          <label><span>W</span><input type="range" min="4" max="24" step="0.1" bind:value={browAlign.w} on:input={() => updateBrows({ w: browAlign.w })} /></label>
          <label><span>H</span><input type="range" min="0.6" max="6" step="0.1" bind:value={browAlign.h} on:input={() => updateBrows({ h: browAlign.h })} /></label>
          <label><span>Gap</span><input type="range" min="6" max="50" step="0.1" bind:value={browAlign.g} on:input={() => updateBrows({ g: browAlign.g })} /></label>
          <label><span>Fill</span><input type="color" bind:value={browColor} on:input={() => setBrowColor(browColor)} /></label>
        </div>
      </div>

      <div class="avatar-controls-footer">
        <button type="button" class="btn" on:click={resetAll}>Reset All</button>
        {#if !detached}
          <small class="muted">Same avatar runtime as the detached window. Drag mouth and brows directly on the image.</small>
        {/if}
      </div>
    </div>
  {/if}

  {#if detached && uiHidden}
    <button type="button" class="avatar-ui-fab" on:click={() => (uiHidden = false)} aria-label="Show controls"></button>
  {/if}
</section>

<style>
  .avatar-runtime {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    gap: 14px;
    min-height: 0;
    align-items: start;
  }

  .avatar-runtime.detached {
    min-height: 100vh;
    padding: 14px;
    background: #000;
  }

  .avatar-stage {
    min-height: 0;
    display: grid;
    place-items: center;
    background: #000;
    padding: 12px;
  }

  .avatar-jar-shell {
    position: relative;
    width: min(100%, 560px);
    display: grid;
    justify-items: center;
    gap: 0;
    filter: drop-shadow(0 24px 42px rgba(0, 0, 0, 0.5));
  }

  .avatar-jar-shell.speaking {
    filter:
      drop-shadow(0 24px 42px rgba(0, 0, 0, 0.52))
      drop-shadow(0 0 18px rgba(138, 178, 255, 0.16));
  }

  .jar-top-rim,
  .jar-bottom-rim {
    width: 92%;
    height: 16px;
    border-radius: 999px;
    background:
      linear-gradient(180deg, rgba(255,255,255,0.4), rgba(216,228,255,0.08)),
      rgba(129, 150, 196, 0.22);
    border: 1px solid rgba(233, 240, 255, 0.28);
    box-shadow:
      inset 0 1px 0 rgba(255,255,255,0.34),
      0 10px 18px rgba(0,0,0,0.16);
  }

  .jar-top-rim {
    margin-bottom: -8px;
    z-index: 4;
  }

  .jar-bottom-rim {
    margin-top: -8px;
    z-index: 4;
  }

  .jar-glass {
    position: relative;
    width: 100%;
    min-height: 320px;
    padding: 26px 18px 22px;
    border-radius: 34px;
    background:
      radial-gradient(120% 100% at 50% 12%, rgba(200, 221, 255, 0.24), transparent 38%),
      linear-gradient(180deg, rgba(170, 192, 235, 0.14), rgba(48, 61, 92, 0.16) 34%, rgba(18, 21, 30, 0.12) 100%);
    border: 1px solid rgba(231, 238, 255, 0.18);
    overflow: hidden;
    box-shadow:
      inset 0 0 0 1px rgba(255,255,255,0.06),
      inset 0 -24px 42px rgba(4, 8, 18, 0.16),
      inset 0 16px 28px rgba(255,255,255,0.08);
  }

  .avatar-wrap {
    position: relative;
    display: grid;
    place-items: center;
    width: 100%;
    min-height: 320px;
    background:
      radial-gradient(78% 88% at 50% 28%, rgba(95, 124, 164, 0.18), transparent 62%),
      linear-gradient(180deg, rgba(0, 0, 0, 0.04), rgba(0, 0, 0, 0.24));
    overflow: hidden;
    border-radius: 26px;
  }

  .avatar-image {
    display: block;
    width: 100%;
    height: auto;
    max-height: min(70vh, 760px);
    object-fit: contain;
    user-select: none;
    pointer-events: none;
  }

  .detached .avatar-image {
    max-height: calc(100vh - 28px);
  }

  .jar-reflection {
    position: absolute;
    pointer-events: none;
    z-index: 3;
    mix-blend-mode: screen;
  }

  .jar-reflection-primary {
    top: 4%;
    left: 8%;
    width: 18%;
    height: 78%;
    border-radius: 999px;
    background: linear-gradient(180deg, rgba(255,255,255,0.42), rgba(255,255,255,0.08) 28%, rgba(255,255,255,0.02) 100%);
    filter: blur(1px);
    opacity: 0.82;
  }

  .jar-reflection-secondary {
    top: 10%;
    right: 9%;
    width: 10%;
    height: 42%;
    border-radius: 999px;
    background: linear-gradient(180deg, rgba(214,229,255,0.26), rgba(214,229,255,0.02));
    opacity: 0.62;
  }

  .jar-reflection-rim {
    top: 0;
    left: 12%;
    width: 76%;
    height: 18%;
    border-radius: 999px;
    background: radial-gradient(ellipse at center, rgba(255,255,255,0.34), rgba(255,255,255,0.02) 68%);
    opacity: 0.9;
  }

  .jar-inner-shadow {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 2;
    background:
      radial-gradient(85% 70% at 50% 100%, rgba(12, 14, 20, 0.28), transparent 56%),
      linear-gradient(90deg, rgba(20, 26, 38, 0.18), transparent 16%, transparent 84%, rgba(20, 26, 38, 0.18));
  }

  .jar-floor-shadow {
    width: 78%;
    height: 32px;
    margin-top: -8px;
    border-radius: 999px;
    background: radial-gradient(ellipse at center, rgba(0,0,0,0.38), rgba(0,0,0,0.04) 70%, transparent 78%);
    filter: blur(6px);
    opacity: 0.8;
  }

  .mouth {
    position: absolute;
    transform: translate(-50%, -50%) scaleY(0.25);
    transform-origin: center;
    border: 0;
    border-radius: 999px;
    box-shadow: 0 0 0 1px rgba(255,255,255,0.08) inset;
    cursor: grab;
    transition: transform 70ms linear;
  }

  .mouth.viseme-rest { transform: translate(-50%, -50%) scaleY(0.22); }
  .mouth.viseme-a { transform: translate(-50%, -50%) scaleY(0.55); }
  .mouth.viseme-e { transform: translate(-50%, -50%) scaleY(0.4); }
  .mouth.viseme-o { transform: translate(-50%, -50%) scaleY(0.66); }
  .mouth.viseme-m { transform: translate(-50%, -50%) scaleY(0.14); }
  .mouth.viseme-f { transform: translate(-50%, -50%) scaleY(0.3); }

  .brows {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .brow {
    position: absolute;
    transform: translate(-50%, -50%);
    border-radius: 999px;
    box-shadow: 0 0 0 1px rgba(0,0,0,0.55) inset, 0 1px 6px rgba(0,0,0,0.55);
    pointer-events: auto;
    cursor: grab;
  }

  .brows.style-straight .left,
  .brows.style-straight .right {
    rotate: 0deg;
  }

  .brows.style-arched .left { rotate: -11deg; }
  .brows.style-arched .right { rotate: 11deg; }
  .brows.style-angry .left { rotate: 14deg; }
  .brows.style-angry .right { rotate: -14deg; }

  .avatar-tools {
    display: grid;
    gap: 10px;
    align-content: start;
    padding: 12px;
    background: rgba(84, 88, 101, 0.86);
  }

  .avatar-tools-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .avatar-tool-actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .avatar-controls-grid {
    display: grid;
    gap: 12px;
  }

  .avatar-control-group {
    display: grid;
    gap: 8px;
  }

  .avatar-group-title {
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: rgba(255,255,255,0.72);
  }

  .avatar-control-group label {
    display: grid;
    grid-template-columns: 48px 1fr;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: rgba(255,255,255,0.76);
  }

  .avatar-controls-footer {
    display: grid;
    gap: 8px;
  }

  .avatar-ui-fab {
    position: fixed;
    left: 12px;
    bottom: 12px;
    z-index: 25;
    width: 22px;
    height: 22px;
    min-width: 22px;
    min-height: 22px;
    padding: 0;
    border-radius: 50%;
    border: 1px solid rgba(129, 163, 124, 0.6);
    background: linear-gradient(180deg, rgba(95, 124, 90, 0.92), rgba(68, 92, 65, 0.95));
  }

  .avatar-ui-fab::before {
    content: '';
    display: block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin: 0 auto;
    background: rgba(205, 226, 198, 0.72);
  }

  @media (max-width: 980px) {
    .avatar-runtime {
      grid-template-columns: 1fr;
    }
  }
</style>
