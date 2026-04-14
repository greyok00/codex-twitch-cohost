<script lang="ts">
  import { Select } from 'bits-ui';

  export let options: Array<{ value: string; label: string; accent?: 'default' | 'gold' }> = [];
  export let value = '';
  export let placeholder = 'Select option';
  export let disabled = false;
  export let fullWidth = true;

  $: items = options.map((opt) => ({ value: opt.value, label: opt.label, accent: opt.accent ?? 'default' }));
  $: selected = items.find((item) => item.value === value);

  function onSelectedChange(next: { value: string; label?: string; accent?: 'default' | 'gold' } | undefined) {
    value = next?.value ?? '';
  }
</script>

<Select.Root items={items} {selected} {disabled} onSelectedChange={onSelectedChange}>
  <Select.Trigger class="ui-select-trigger {fullWidth ? 'full' : 'compact'}" aria-label={placeholder}>
    <Select.Value class="ui-select-value {selected?.accent === 'gold' ? 'gold' : ''}" {placeholder} />
  </Select.Trigger>
  <Select.Content class="ui-select-content" sideOffset={6}>
    {#each items as item}
      <Select.Item class="ui-select-item" value={item.value} label={item.label}>
        <span class:gold={item.accent === 'gold'}>{item.label}</span>
        <Select.ItemIndicator class="ui-select-indicator">✓</Select.ItemIndicator>
      </Select.Item>
    {/each}
  </Select.Content>
</Select.Root>

<style>
  .gold {
    color: #d4a937;
    font-weight: 700;
  }
</style>
