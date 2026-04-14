<script lang="ts">
  import { Slider } from 'bits-ui';

  export let value = 0;
  export let min = 0;
  export let max = 100;
  export let step = 1;
  export let disabled = false;
  export let ariaLabel = 'Slider';

  function onValueChange(next: number[]) {
    value = Array.isArray(next) && next.length > 0 ? next[0] : value;
  }
</script>

<Slider.Root
  class="ui-slider"
  {min}
  {max}
  {step}
  value={[value]}
  {disabled}
  onValueChange={onValueChange}
  aria-label={ariaLabel}
  let:thumbs
>
  <span class="ui-slider-track">
    <Slider.Range class="ui-slider-range" />
  </span>
  {#each thumbs as thumb}
    <Slider.Thumb {thumb} class="ui-slider-thumb" />
  {/each}
</Slider.Root>
