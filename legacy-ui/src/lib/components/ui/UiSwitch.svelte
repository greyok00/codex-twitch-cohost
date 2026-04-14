<script lang="ts">
  import { Switch } from 'bits-ui';

  export let checked = false;
  export let disabled = false;
  export let ariaLabel = 'Toggle';
  export let onCheckedChange: ((next: boolean) => void) | undefined = undefined;

  function handleCheckedChange(next: boolean) {
    checked = !!next;
    onCheckedChange?.(checked);
  }
</script>

<Switch.Root class="ui-switch" {checked} {disabled} onCheckedChange={handleCheckedChange} aria-label={ariaLabel}>
  <Switch.Thumb class="ui-switch-thumb" />
</Switch.Root>
