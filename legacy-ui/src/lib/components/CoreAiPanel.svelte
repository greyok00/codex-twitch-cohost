<script lang="ts">
  import CloudSetupCard from './CloudSetupCard.svelte';
  import MemoryPanel from './MemoryPanel.svelte';
</script>

<section class="grid">
  <section class="card grid">
    <h3>Core AI</h3>
    <small class="muted">Model selection, memory policy, and future tool rules live here. This stays separate from voice and separate from character packaging.</small>
  </section>
  <CloudSetupCard />
  <MemoryPanel />
</section>
