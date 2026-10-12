<script lang="ts">
  let { name, size = 16, stroke = 1.6 }: { name: string; size?: number; stroke?: number } = $props();
</script>
<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={stroke} stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === 'folder'}<path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
  {:else if name === 'folder-open'}<path d="M3 17V7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v2M3 19l3-7h16l-3 7a2 2 0 0 1-2 1H5a2 2 0 0 1-2-1Z" />
  {:else if name === 'repo'}<path d="M4 5a2 2 0 0 1 2-2h14v18H6a2 2 0 0 1-2-2Zm0 12h16M8 7h8M8 11h5" />
  {:else if name === 'branch'}<circle cx="6" cy="5" r="2" /><circle cx="18" cy="6" r="2" /><circle cx="6" cy="19" r="2" /><path d="M6 7v10M18 8v2a6 6 0 0 1-6 6H6" />
  {:else if name === 'file'}<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9Zm0 0v6h6" />
  {:else if name === 'code'}<path d="m8 8-4 4 4 4m8-8 4 4-4 4m-3-11-2 14" />
  {:else if name === 'search'}<circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" />
  {:else if name === 'plus'}<path d="M12 5v14M5 12h14" />
  {:else if name === 'edit'}<path d="m15 5 4 4M4 20l5-1L21 7a3 3 0 0 0-4-4L5 15ZM4 20h16" />
  {:else if name === 'refresh'}<path d="M20 7a8 8 0 1 0 1 7M20 3v5h-5" />
  {:else if name === 'chevron-right'}<path d="m9 6 6 6-6 6" />
  {:else if name === 'chevron-down'}<path d="m6 9 6 6 6-6" />
  {:else if name === 'chevron-up'}<path d="m6 15 6-6 6 6" />
  {:else if name === 'close'}<path d="m6 6 12 12M6 18 18 6" />
  {:else if name === 'copy'}<rect x="8" y="8" width="13" height="13" rx="2" /><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3" />
  {:else if name === 'check'}<path d="m5 12 4 4L19 6" />
  {:else if name === 'eye'}<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z" /><circle cx="12" cy="12" r="3" />
  {:else if name === 'eye-off'}<path d="m3 3 18 18M9.9 5.2A12 12 0 0 1 12 5c6.5 0 10 7 10 7a18 18 0 0 1-3 4M6 6a18 18 0 0 0-4 6s3.5 7 10 7a12 12 0 0 0 6-1.7M10 10a3 3 0 0 0 4 4" />
  {:else if name === 'warning'}<path d="m12 3 10 18H2ZM12 9v4m0 4h.01" />
  {:else if name === 'split'}<rect x="3" y="4" width="18" height="16" rx="3" /><path d="M12 4v16m-5-8h2m6 0h2" />
  {:else if name === 'clock'}<circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" />
  {:else if name === 'collapse'}<path d="M7 3h14v14M3 7h14v14H3Zm4 7h6" />
  {:else if name === 'external'}<path d="M14 3h7v7m0-7L10 14m0-11H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5" />
  {:else if name === 'spark'}<path d="m12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5Z" />
  {/if}
</svg>
