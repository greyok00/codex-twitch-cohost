<script lang="ts">
  import { Select } from 'bits-ui';

  export let options: Array<{ value: string; label: string }> = [];
  export let value = '';
  export let placeholder = 'Select option';
  export let disabled = false;
  export let fullWidth = true;

  $: items = options.map((opt) => ({ value: opt.value, label: opt.label }));
  $: selected = items.find((item) => item.value === value);

  function onSelectedChange(next: { value: string; label?: string } | undefined) {
    value = next?.value ?? '';
  }
</script>

<Select.Root items={items} {selected} {disabled} onSelectedChange={onSelectedChange}>
  <Select.Trigger class="ui-select-trigger {fullWidth ? 'full' : 'compact'}" aria-label={placeholder}>
    <Select.Value class="ui-select-value" {placeholder} />
  </Select.Trigger>
  <Select.Content class="ui-select-content" sideOffset={6}>
    {#each items as item}
      <Select.Item class="ui-select-item" value={item.value} label={item.label}>
        <span>{item.label}</span>
        <Select.ItemIndicator class="ui-select-indicator">✓</Select.ItemIndicator>
      </Select.Item>
    {/each}
  </Select.Content>
</Select.Root>
