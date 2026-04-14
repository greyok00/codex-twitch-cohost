<script lang="ts">
  export let name:
    | 'external'
    | 'key'
    | 'book'
    | 'cloud'
    | 'summary'
    | 'reset'
    | 'avatar'
    | 'mic'
    | 'send'
    | 'spark'
    | 'save'
    | 'check'
    | 'wrench'
    | 'copy'
    | 'folder'
    | 'close'
    | 'bot'
    | 'user'
    | 'plug'
    | 'unplug'
    | 'switch'
    | 'trash'
    | 'chevron'
    | 'shield'
    | 'voice'
    | 'pause'
    | 'play' = 'spark';
  export let size = 15;
</script>

<svg class="btn-ico" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === 'external'}
    <path d="M14 4h6v6" />
    <path d="M10 14 20 4" />
    <path d="M20 14v6h-16v-16h6" />
  {:else if name === 'key'}
    <circle cx="8.5" cy="12" r="3.5" />
    <path d="M12 12h8" />
    <path d="M17 12v3" />
    <path d="M20 12v2" />
  {:else if name === 'book'}
    <path d="M4 5a3 3 0 0 1 3-3h13v18h-13a3 3 0 0 0-3 3z" />
    <path d="M7 2v18" />
  {:else if name === 'cloud'}
    <path d="M7 18h10a4 4 0 1 0-.8-7.9A5.5 5.5 0 0 0 5.4 11.8 3.8 3.8 0 0 0 7 18z" />
  {:else if name === 'summary'}
    <path d="M4 6h16" />
    <path d="M4 12h12" />
    <path d="M4 18h8" />
  {:else if name === 'reset'}
    <path d="M20 6v6h-6" />
    <path d="M20 12a8 8 0 1 1-2.3-5.7" />
  {:else if name === 'avatar'}
    <circle cx="12" cy="8" r="3.2" />
    <path d="M5 20c1.8-3.7 4.2-5.5 7-5.5s5.2 1.8 7 5.5" />
  {:else if name === 'mic'}
    <rect x="9" y="3" width="6" height="11" rx="3" />
    <path d="M6 11a6 6 0 0 0 12 0" />
    <path d="M12 17v4" />
  {:else if name === 'send'}
    <path d="M3 11.5 21 3l-6.5 18-2.8-6.7z" />
  {:else if name === 'spark'}
    <path d="M12 3l1.8 4.2L18 9l-4.2 1.8L12 15l-1.8-4.2L6 9l4.2-1.8z" />
    <path d="M19 14l.9 2.1L22 17l-2.1.9L19 20l-.9-2.1L16 17l2.1-.9z" />
  {:else if name === 'save'}
    <path d="M4 4h13l3 3v13H4z" />
    <path d="M8 4v6h8V4" />
    <path d="M8 20v-6h8v6" />
  {:else if name === 'check'}
    <path d="m4 12 5 5 11-11" />
  {:else if name === 'wrench'}
    <path d="M14 7a4 4 0 0 0 5 5l-9.5 9.5a2 2 0 1 1-2.8-2.8L16.2 9.2A4 4 0 0 0 14 7z" />
  {:else if name === 'copy'}
    <rect x="9" y="9" width="11" height="11" rx="2" />
    <path d="M5 15V5h10" />
  {:else if name === 'folder'}
    <path d="M3 7h6l2 2h10v8a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3z" />
    <path d="M3 10V7a3 3 0 0 1 3-3h3l2 2h7a3 3 0 0 1 3 3v1" />
  {:else if name === 'close'}
    <path d="m6 6 12 12" />
    <path d="m18 6-12 12" />
  {:else if name === 'bot'}
    <rect x="5" y="7" width="14" height="11" rx="3" />
    <path d="M9 12h.01M15 12h.01" />
    <path d="M12 3v4" />
  {:else if name === 'user'}
    <circle cx="12" cy="8" r="3.2" />
    <path d="M5 20c1.8-3.7 4.2-5.5 7-5.5s5.2 1.8 7 5.5" />
  {:else if name === 'plug'}
    <path d="M8 7v5a4 4 0 0 0 8 0V7" />
    <path d="M10 3v4M14 3v4" />
    <path d="M12 16v5" />
  {:else if name === 'unplug'}
    <path d="M8 7v5a4 4 0 0 0 8 0V7" />
    <path d="M10 3v4M14 3v4" />
    <path d="M12 16v5" />
    <path d="m4 4 16 16" />
  {:else if name === 'switch'}
    <path d="M17 3h4v4" />
    <path d="M21 3 14 10" />
    <path d="M7 21H3v-4" />
    <path d="M3 21 10 14" />
  {:else if name === 'trash'}
    <path d="M4 7h16" />
    <path d="M10 3h4" />
    <path d="M7 7l1 14h8l1-14" />
  {:else if name === 'chevron'}
    <path d="m8 10 4 4 4-4" />
  {:else if name === 'shield'}
    <path d="M12 3 5 6v6c0 4.2 2.9 7.7 7 9 4.1-1.3 7-4.8 7-9V6z" />
  {:else if name === 'voice'}
    <path d="M12 3v6" />
    <path d="M8.5 8.5a4.9 4.9 0 0 0 0 7M15.5 8.5a4.9 4.9 0 0 1 0 7" />
    <path d="M5.5 5.5a9 9 0 0 0 0 13M18.5 5.5a9 9 0 0 1 0 13" />
  {:else if name === 'play'}
    <path d="M8 5v14l11-7z" />
  {:else if name === 'pause'}
    <path d="M8 5h3v14H8z" />
    <path d="M13 5h3v14h-3z" />
  {/if}
</svg>
