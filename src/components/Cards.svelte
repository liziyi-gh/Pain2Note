<script lang="ts">
  let { cards, small = false }: { cards: string[]; small?: boolean } = $props();
  const suits: Record<string, string> = { s: '♠', h: '♥', d: '♦', c: '♣' };
</script>

<span class:small class="playing-cards">
  {#each cards as card}<span
      class="playing-card suit-{card[1]}"
      aria-label={card}>{card[0]}<span>{suits[card[1]]}</span></span
    >{/each}
</span>
