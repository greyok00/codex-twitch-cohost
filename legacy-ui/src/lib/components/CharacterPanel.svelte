<script lang="ts">
  import PersonalityEditor from './PersonalityEditor.svelte';
  import AvatarPanel from './AvatarPanel.svelte';
</script>

<section class="grid">
  <section class="grid two-stack">
    <div class="grid">
      <small class="muted">AI Identity</small>
      <PersonalityEditor />
    </div>
    <div class="grid">
      <small class="muted">Avatar</small>
      <AvatarPanel />
    </div>
  </section>
</section>
