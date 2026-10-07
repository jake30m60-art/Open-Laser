<script lang="ts">
  import { t } from '../i18n/index';
  import Modal from './Modal.svelte';
  import { access } from '../lib/access.svelte';

  const discovery = $derived(access.info?.discovery.error ?? (
    access.info?.discovery.ready ? t('Open this address on any phone, tablet or computer connected to the same network.')
    : access.info?.discovery.enabled ? t('Announcing the local address…')
    : t('Local network sharing is off.')));
</script>

<Modal title={t("Local network")} onclose={() => access.manage = false}>
  <div class="network-address">
    <span>{t("OpenLaser on your network")}</span>
    <strong>{access.info?.address.replace('http://', '')}</strong>
    <p>{discovery}</p>
  </div>
</Modal>

<style>
  .network-address { display:grid; gap:14px; }
  .network-address span { font-size:var(--t-sm); color:var(--ink-2); }
  .network-address strong { font-size:var(--t-xl); letter-spacing:-.02em; overflow-wrap:anywhere; }
  .network-address p { color:var(--ink-3); font-size:var(--t-base); line-height:1.6; }
</style>
