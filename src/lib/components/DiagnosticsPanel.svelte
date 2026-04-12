<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from 'bits-ui';
  import Icon from './ui/Icon.svelte';
  import { errorBannerStore, debugBundleStore, selfTestReportStore, serviceHealthStore } from '../stores/app';
  import { exportDebugBundle, getServiceHealth, runSelfTest, verifyVoiceRuntime } from '../api/tauri';
  import type { VoiceRuntimeReport } from '../types';

  export let embedded = false;

  let loadingHealth = false;
  let runningSelfTest = false;
  let exporting = false;
  let checkingVoice = false;
  let voiceReport: VoiceRuntimeReport | null = null;

  onMount(() => {
    void refreshHealth();
  });

  async function refreshHealth() {
    loadingHealth = true;
    try {
      await getServiceHealth();
    } catch (error) {
      errorBannerStore.set('Service health load failed: ' + String(error));
    } finally {
      loadingHealth = false;
    }
  }

  async function runHealthTest() {
    runningSelfTest = true;
    try {
      await runSelfTest();
      await refreshHealth();
    } catch (error) {
      errorBannerStore.set('Self-test failed: ' + String(error));
    } finally {
      runningSelfTest = false;
    }
  }

  async function runVoiceCheck() {
    checkingVoice = true;
    try {
      voiceReport = await verifyVoiceRuntime();
      await refreshHealth();
    } catch (error) {
      errorBannerStore.set('Voice verification failed: ' + String(error));
    } finally {
      checkingVoice = false;
    }
  }

  async function exportBundle() {
    exporting = true;
    try {
      await exportDebugBundle();
    } catch (error) {
      errorBannerStore.set('Debug bundle export failed: ' + String(error));
    } finally {
      exporting = false;
    }
  }
</script>

<section class:card={!embedded} class="grid diagnostics-panel">
  <div class="row wrap">
    <h3>{embedded ? 'Diagnostics & Self-Test' : 'Diagnostics'}</h3>
    <div class="actions">
      <Button.Root class="p-btn btn" on:click={refreshHealth} disabled={loadingHealth}>
        <Icon name="check" />{loadingHealth ? 'Refreshing...' : 'Refresh Health'}
      </Button.Root>
      <Button.Root class="p-btn btn" on:click={runHealthTest} disabled={runningSelfTest}>
        <Icon name="shield" />{runningSelfTest ? 'Running Self-Test...' : 'Run Self-Test'}
      </Button.Root>
      <Button.Root class="p-btn btn" on:click={runVoiceCheck} disabled={checkingVoice}>
        <Icon name="voice" />{checkingVoice ? 'Checking Voice...' : 'Verify STT/TTS'}
      </Button.Root>
      <Button.Root class="p-btn btn" on:click={exportBundle} disabled={exporting}>
        <Icon name="save" />{exporting ? 'Exporting...' : 'Export Debug Bundle'}
      </Button.Root>
    </div>
  </div>

  {#if $serviceHealthStore}
    <small class="muted">
      Service health: <strong class={$serviceHealthStore.overall}>{$serviceHealthStore.overall.toUpperCase()}</strong>
      | Generated: {$serviceHealthStore.generatedAt}
    </small>
    <div class="service-grid">
      {#each $serviceHealthStore.services as service}
        <article class="card-lite service-card">
          <div class="row">
            <strong>{service.label}</strong>
            <span class="chip {service.status === 'pass' ? 'ok' : service.status === 'warn' ? 'warn' : 'bad'}">{service.status}</span>
          </div>
          <small>Configured: {service.configured ? 'yes' : 'no'}</small>
          <small>Available: {service.available ? 'yes' : 'no'}</small>
          <small>Authenticated: {service.authenticated ? 'yes' : 'no'}</small>
          <small>Active: {service.active ? 'yes' : 'no'}</small>
          {#each service.details as detail}
            <small class="muted">{detail}</small>
          {/each}
        </article>
      {/each}
    </div>
  {/if}

  {#if $selfTestReportStore}
    <div class="checks">
      <small class="muted">
        Self-test: <strong class={$selfTestReportStore.overall}>{$selfTestReportStore.overall.toUpperCase()}</strong>
        | Generated: {$selfTestReportStore.generatedAt}
      </small>
      {#each $selfTestReportStore.checks as check}
        <small class="check {check.status}">
          [{check.status.toUpperCase()}] {check.name}: {check.details}
        </small>
      {/each}
    </div>
  {/if}

  {#if voiceReport}
    <div class="checks">
      <small class="muted">
        Voice runtime: <strong class={voiceReport.overall}>{voiceReport.overall.toUpperCase()}</strong>
        | Generated: {voiceReport.generatedAt}
      </small>
      {#each voiceReport.checks as check}
        <small class="check {check.status}">
          [{check.status.toUpperCase()}] {check.name}: {check.details}
        </small>
      {/each}
    </div>
  {/if}

  {#if $debugBundleStore}
    <small class="muted">
      Debug bundle written to: {$debugBundleStore.path}
    </small>
  {/if}
</section>
