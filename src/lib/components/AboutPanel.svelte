<script lang="ts">
  import { onMount } from 'svelte';

  const storageKey = 'cohost.creator.about.v1';

  let creatorName = 'Your Name';
  let creatorTagline = 'AI tools creator and streamer builder';
  let creatorBio = 'Built to make live streams faster, smarter, and more entertaining with an always-on cohost bot.';
  let links = 'https://twitch.tv/yourchannel\nhttps://github.com/yourname';
  let saved = false;

  onMount(() => {
    try {
      const raw = localStorage.getItem(storageKey);
      if (!raw) return;
      const parsed = JSON.parse(raw) as {
        creatorName?: string;
        creatorTagline?: string;
        creatorBio?: string;
        links?: string;
      };
      creatorName = parsed.creatorName || creatorName;
      creatorTagline = parsed.creatorTagline || creatorTagline;
      creatorBio = parsed.creatorBio || creatorBio;
      links = parsed.links || links;
    } catch {
      // Ignore malformed local settings.
    }
  });

  function save() {
    localStorage.setItem(
      storageKey,
      JSON.stringify({
        creatorName,
        creatorTagline,
        creatorBio,
        links
      })
    );
    saved = true;
    setTimeout(() => (saved = false), 1400);
  }
</script>

<section class="card grid about-card">
  <h3>ℹ️ About This App</h3>
  <small class="muted">Creator profile shown to users and collaborators.</small>

  <div class="preview card">
    <h4>{creatorName}</h4>
    <p>{creatorTagline}</p>
    <small>{creatorBio}</small>
    <div class="link-list">
      {#each links.split('\n').filter((s) => s.trim().length > 0) as link}
        <span class="chip">{link.trim()}</span>
      {/each}
    </div>
  </div>

  <label for="creator-name">Creator name</label>
  <input id="creator-name" bind:value={creatorName} placeholder="Your name" />

  <label for="creator-tagline">Tagline</label>
  <input id="creator-tagline" bind:value={creatorTagline} placeholder="Short one-liner" />

  <label for="creator-bio">Bio</label>
  <textarea id="creator-bio" bind:value={creatorBio} rows="3" placeholder="What this project is and who built it."></textarea>

  <label for="creator-links">Links (one per line)</label>
  <textarea id="creator-links" bind:value={links} rows="4" placeholder="https://...\nhttps://..."></textarea>

  <div class="actions">
    <button class="btn" on:click={save}>Save About Profile</button>
    {#if saved}<small class="ok">Saved</small>{/if}
  </div>
</section>

<style>
  .about-card {
    max-width: 920px;
  }
  .preview {
    border-radius: 12px;
    padding: 0.9rem;
  }
  h4 {
    margin: 0;
    font-size: 1.2rem;
  }
  .preview p {
    margin: 0.3rem 0;
  }
  .link-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-top: 0.5rem;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .ok {
    color: var(--ok);
    font-weight: 700;
  }
</style>
