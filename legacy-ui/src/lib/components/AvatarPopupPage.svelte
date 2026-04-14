<script lang="ts">
  import AvatarRuntime from './AvatarRuntime.svelte';
</script>

<div class="avatar-popup-page">
  <AvatarRuntime detached={true} />
</div>

<style>
  .avatar-popup-page {
    min-height: 100dvh;
    background: #000;
  }
</style>
