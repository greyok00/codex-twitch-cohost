<script lang="ts">
  import { Button } from 'bits-ui';
  import Icon from './ui/Icon.svelte';
  import { runSelfTest } from '../api/tauri';
  import { errorBannerStore, selfTestReportStore } from '../stores/app';

  let running = false;

  async function run() {
    running = true;
    try {
      await runSelfTest();
    } catch (error) {
      errorBannerStore.set('Self-test failed: ' + String(error));
    } finally {
      running = false;
    }
  }
</script>

<section class="card grid">
  <div class="row">
    <h3>Health Self-Test</h3>
    <Button.Root class="p-btn btn" on:click={run} disabled={running}><Icon name="shield" />{running ? 'Running...' : 'Run Self-Test'}</Button.Root>
  </div>

  {#if $selfTestReportStore}
    <small class="muted">
      Overall: <strong class={$selfTestReportStore.overall}>{$selfTestReportStore.overall.toUpperCase()}</strong>
      | Generated: {$selfTestReportStore.generatedAt}
    </small>

    <div class="checks">
      {#each $selfTestReportStore.checks as check}
        <div class="check {check.status}">
          <strong>{check.name}</strong>
          <span>{check.details}</span>
        </div>
      {/each}
    </div>
  {:else}
    <small class="muted">Run self-test to verify auth/session/chat/eventsub/provider health.</small>
  {/if}
</section>
