<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  const dispatch = createEventDispatcher<{ complete: void }>();

  function complete() {
    localStorage.setItem('cohost.onboarded.v1', 'true');
    dispatch('complete');
  }
</script>

<section class="welcome-wrap">
  <div class="orb one"></div>
  <div class="orb two"></div>
  <div class="card welcome">
    <h1>🎬 Twitch Cohost Bot</h1>
    <p>First-time setup in under a minute.</p>
    <ol>
      <li>Connect your bot Twitch account</li>
      <li>Connect your streamer account</li>
      <li>Paste your Ollama Cloud API key</li>
      <li>Click Connect Chat</li>
    </ol>
    <div class="actions">
      <button class="btn" on:click={complete}>Start Setup</button>
    </div>
    <small class="muted">You can reopen this later from local storage by clearing `cohost.onboarded.v1`.</small>
  </div>
</section>

<style>
  .welcome-wrap {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    background: radial-gradient(circle at 15% 20%, rgba(200, 154, 99, 0.24), transparent 42%), rgba(8, 6, 5, 0.84);
    backdrop-filter: blur(3px);
    overflow: hidden;
  }
  .welcome {
    width: min(680px, 92vw);
    animation: welcomeIn 420ms ease both;
  }
  h1 {
    margin: 0;
    font-size: 1.9rem;
  }
  p {
    margin: 0.4rem 0 0.2rem;
    color: var(--muted);
  }
  ol {
    margin: 0.2rem 0 0;
    display: grid;
    gap: 0.35rem;
  }
  .actions {
    display: flex;
    gap: 0.6rem;
    margin-top: 0.6rem;
  }
  .orb {
    position: absolute;
    width: 340px;
    height: 340px;
    border-radius: 999px;
    filter: blur(56px);
    pointer-events: none;
    opacity: 0.55;
  }
  .orb.one {
    background: #b37a43;
    top: -80px;
    left: -50px;
    animation: floatOne 6s ease-in-out infinite;
  }
  .orb.two {
    background: #d8ad76;
    right: -60px;
    bottom: -100px;
    animation: floatTwo 8s ease-in-out infinite;
  }
  @keyframes welcomeIn {
    from {
      opacity: 0;
      transform: translateY(14px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
  @keyframes floatOne {
    0%,
    100% {
      transform: translate(0, 0);
    }
    50% {
      transform: translate(20px, 22px);
    }
  }
  @keyframes floatTwo {
    0%,
    100% {
      transform: translate(0, 0);
    }
    50% {
      transform: translate(-24px, -16px);
    }
  }
</style>
