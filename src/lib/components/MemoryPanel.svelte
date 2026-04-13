<script lang="ts">
  import { Button } from 'bits-ui';
  import { clearMemory, deletePinnedMemory, getMemorySnapshot, openMemoryLog, summarizeChat, upsertPinnedMemory } from '../api/tauri';
  import Icon from './ui/Icon.svelte';
  import type { MemorySnapshot } from '../types';
  import { errorBannerStore, eventStore } from '../stores/app';

  let summary = '';
  let snapshot: MemorySnapshot | null = null;
  let pinLabel = '';
  let pinContent = '';

  async function refreshMemory() {
    try {
      snapshot = await getMemorySnapshot();
    } catch (error) {
      errorBannerStore.set('Memory load failed: ' + String(error));
    }
  }

  async function summarize() {
    try {
      summary = await summarizeChat();
    } catch (error) {
      errorBannerStore.set('Summary failed: ' + String(error));
    }
  }

  async function wipe() {
    try {
      await clearMemory();
      summary = 'Memory cleared.';
      await refreshMemory();
    } catch (error) {
      errorBannerStore.set('Memory clear failed: ' + String(error));
    }
  }

  async function copyPath() {
    if (!snapshot?.logPath) return;
    try {
      await navigator.clipboard.writeText(snapshot.logPath);
      summary = 'Memory log path copied.';
    } catch (error) {
      errorBannerStore.set('Memory path copy failed: ' + String(error));
    }
  }

  async function openLog() {
    try {
      await openMemoryLog();
    } catch (error) {
      errorBannerStore.set('Memory log open failed: ' + String(error));
    }
  }

  async function savePinned() {
    const label = pinLabel.trim();
    const content = pinContent.trim();
    if (!label || !content) {
      summary = 'Pinned memory needs both a label and content.';
      return;
    }
    try {
      await upsertPinnedMemory(label, content);
      summary = `Pinned memory saved for ${label}.`;
      pinLabel = '';
      pinContent = '';
      await refreshMemory();
    } catch (error) {
      errorBannerStore.set('Pinned memory save failed: ' + String(error));
    }
  }

  async function removePinned(label: string) {
    try {
      await deletePinnedMemory(label);
      summary = `Removed pinned memory for ${label}.`;
      await refreshMemory();
    } catch (error) {
      errorBannerStore.set('Pinned memory delete failed: ' + String(error));
    }
  }

  void refreshMemory();
</script>

<section class="card grid">
  <h3>Memory</h3>
  <div class="row">
    <Button.Root class="p-btn" on:click={summarize}><Icon name="summary" />Summarize Chat</Button.Root>
    <Button.Root class="p-btn" on:click={refreshMemory}><Icon name="copy" />Refresh Memory</Button.Root>
    <Button.Root class="p-btn" on:click={wipe}><Icon name="trash" />Reset Memory</Button.Root>
  </div>
  {#if summary}
    <p>{summary}</p>
  {/if}
  {#if snapshot}
    <div class="grid">
      <small class="muted">Memory log file: {snapshot.logPath}</small>
      <div class="row">
        <Button.Root class="p-btn" on:click={copyPath}><Icon name="copy" />Copy Path</Button.Root>
        <Button.Root class="p-btn" on:click={openLog}><Icon name="folder" />Open Log</Button.Root>
      </div>
    </div>
    <div class="grid">
      <small class="muted">Pinned memory overrides raw chat log and is injected first into context.</small>
      <input bind:value={pinLabel} placeholder="Label: nickname, relationship, hard fact" maxlength="40" />
      <textarea bind:value={pinContent} rows="3" placeholder="What the bot must remember exactly"></textarea>
      <div class="row">
        <Button.Root class="p-btn" on:click={savePinned}><Icon name="save" />Save Pinned Memory</Button.Root>
      </div>
      {#if snapshot.pinned.length > 0}
        <div class="feed">
          {#each snapshot.pinned as item (item.id)}
            <div class="line">
              <span class="tag">pinned</span>
              <strong>{item.label}</strong>
              <span>{item.content}</span>
              <Button.Root class="p-btn" on:click={() => removePinned(item.label)}><Icon name="trash" />Remove</Button.Root>
            </div>
          {/each}
        </div>
      {/if}
    </div>
    <div class="feed">
      {#if snapshot.recent.length === 0}
        <small class="muted">No memory written yet.</small>
      {:else}
        {#each snapshot.recent as item (item.id)}
          <div class="line">
            <span class="tag">{item.kind}</span>
            <strong>{item.user || 'system'}</strong>
            <span>{item.content}</span>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
  <small class="muted">Event cache: {$eventStore.length}</small>
</section>
