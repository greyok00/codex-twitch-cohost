<script lang="ts">
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
    <h3>✅ Health Self-Test</h3>
    <button class="btn" on:click={run} disabled={running}>{running ? 'Running...' : 'Run Self-Test'}</button>
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

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.7rem;
  }
  .checks {
    display: grid;
    gap: 0.45rem;
  }
  .check {
    display: grid;
    gap: 0.2rem;
    padding: 0.55rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: color-mix(in srgb, var(--panel-strong), transparent 8%);
  }
  .check.pass strong {
    color: var(--ok);
  }
  .check.warn strong {
    color: var(--accent);
  }
  .check.fail strong {
    color: var(--danger);
  }
  .pass {
    color: var(--ok);
  }
  .warn {
    color: var(--accent);
  }
  .fail {
    color: var(--danger);
  }
</style>
