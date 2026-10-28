<script lang="ts">
  import Cards from './Cards.svelte';
  import { replayHand } from '../lib/replay';
  import { money, type Hand } from '../lib/types';
  let { hand }: { hand: Hand } = $props();
  let steps = $derived(replayHand(hand));
  let index = $state(0);
  let current = $derived(steps[index]);
  let streets = $derived(
    steps.flatMap((step, i) =>
      i === 0 || step.street !== steps[i - 1].street
        ? [{ name: step.street, index: i }]
        : [],
    ),
  );
</script>

<section class="replay" aria-label="手牌回放">
  {#if current}
    <div class="replay-heading">
      <div><strong>手牌回放</strong><small>按原始牌谱顺序逐步回看</small></div>
      <span>{index} / {steps.length - 1}</span>
    </div>
    <nav class="replay-streets" aria-label="跳转街道">
      {#each streets as item}
        <button
          class:active={current.street === item.name}
          aria-current={current.street === item.name ? 'step' : undefined}
          onclick={() => (index = item.index)}>{item.name}</button
        >
      {/each}
    </nav>
    <div class="replay-felt">
      <div class="replay-pot">
        累计底池（退回后，扣费前） <strong
          >{money(current.pot, hand.currency)}</strong
        >
      </div>
      <div class="replay-boards">
        {#each current.boards as board, i}
          <div>
            {#if current.boards.length > 1}<span>第 {i + 1} 次</span
              >{/if}{#if board.length}<Cards cards={board} small />{:else}<span
                >等待公共牌</span
              >{/if}
          </div>
        {/each}
      </div>
      <div class="replay-players">
        {#each current.players as player}
          <div class:folded={player.folded} class="replay-player">
            <div>
              <b>{player.seat} · {player.name}</b
              >{#if player.name === hand.hero}<em> HERO</em>{/if}
            </div>
            <div>
              {money(player.stack, hand.currency)}
              {#if player.folded}· 弃牌{:else if player.allIn}· 全下{/if}
            </div>
            <div>
              本街 {money(player.bet, hand.currency)}
              {#if player.cards.length}<Cards
                  cards={player.cards}
                  small
                />{:else}<span>底牌未公开</span>{/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
    <div class="replay-action" aria-live="polite">
      <span>{current.street}</span><strong>{current.description}</strong>
    </div>
    <div class="replay-controls">
      <button onclick={() => (index = 0)} disabled={index === 0}
        >回到开始</button
      >
      <button onclick={() => index--} disabled={index === 0}>上一步</button>
      <button onclick={() => index++} disabled={index === steps.length - 1}
        >下一步</button
      >
      <button
        onclick={() => (index = steps.length - 1)}
        disabled={index === steps.length - 1}>跳到最后</button
      >
    </div>
  {:else}
    <p class="replay-unavailable">此手没有可回放的原始牌谱。</p>
  {/if}
</section>
