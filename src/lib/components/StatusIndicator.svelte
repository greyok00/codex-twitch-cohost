<script lang="ts">
  export let label: string;
  export let state: 'connected' | 'connecting' | 'disconnected' | 'error';

  const colors: Record<string, string> = {
    connected: 'var(--ok)',
    connecting: 'var(--accent)',
    disconnected: 'var(--muted)',
    error: 'var(--danger)'
  };
</script>

<div class="status">
  <span class="dot" style={`background:${colors[state] || 'var(--muted)'}`}></span>
  <span>{label}: {state}</span>
</div>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.92rem;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 999px;
  }
</style>
