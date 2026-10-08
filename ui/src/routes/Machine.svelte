<script lang="ts">
  import { t } from '../i18n/index';
  import { chooseLocale, storedLocale } from '../i18n/browser';
  const locale = storedLocale();

  // Settings: one sidebar of sections with search, Advanced last. Display,
  // checklist, route and gas settings save as they change; only what writes
  // the machine (the backup, process INI and controller parameters) waits in
  // Pending changes for a confirmation. Machine tests and the mode switch live
  // beside the jog pad (components/MachineTools.svelte), not here.
  import { onDestroy, tick } from 'svelte';
  import PreflightEditor from '../components/PreflightEditor.svelte';
  import MatrixCalibration from '../components/MatrixCalibration.svelte';
  import ScanCalibration from '../components/ScanCalibration.svelte';
  import MachineSettings from '../components/MachineSettings.svelte';
  import MachineFiles from '../components/MachineFiles.svelte';
  import GasCosts from '../components/GasCosts.svelte';
  import MachineTools from '../components/MachineTools.svelte';
  import { holdSeconds, MAX_HOLD_MS, MIN_HOLD_MS } from '../lib/hold-confirm';
  import { SETTINGS_PAGES } from '../lib/navigation';
  import { APP_VERSION } from '../lib/version';
  import { settingsEdits } from '../lib/settings-edits.svelte';
  import { access, rememberLayout } from '../lib/access.svelte';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { osk } from '../lib/osk.svelte';
  import { explain, laserLabel } from '../lib/format';
  import { units, distance, displayNumber, quantity, unitLabel } from '../lib/units.svelte';

  let { compact = false, sectionOnly = false }: { compact?: boolean; sectionOnly?: boolean } = $props();
  const doc = $derived(server.doc!);
  const machine = $derived(doc.machine);
  const feedback = $derived(machine.feedback);
  const bindings = $derived(doc.bindings);
  const connection = $derived(machine.connection);
  const link = $derived(doc.link);

  const page = $derived(SETTINGS_PAGES.find(page => page.id === ui.machinePage) ?? SETTINGS_PAGES[0]!);
  const query = $derived(ui.settingsQuery.trim().toLocaleLowerCase());
  onDestroy(() => { ui.checklistEditor = null; });

  async function run(action: () => Promise<unknown>, done?: string): Promise<void> {
    try {
      await action();
      if (done) ui.say(done);
    } catch (error) { ui.say(explain(error), true); }
  }

  // Hold times are one setting for every screen; they save at once.
  function holdTime(key: 'move_ms' | 'zero_ms', label: string): void {
    osk.number(label, doc.hold[key] / 1000, 's', (seconds) => {
      const ms = Math.round(seconds * 1000);
      if (ms < MIN_HOLD_MS || ms > MAX_HOLD_MS) {
        ui.say(t("Use {0} to {1}.", holdSeconds(MIN_HOLD_MS), holdSeconds(MAX_HOLD_MS)), true);
        return;
      }
      const saved = $state.snapshot(doc.hold);
      void run(() => api.saveHoldTimes({ ...saved, [key]: ms }, saved), t("Hold time saved."));
    });
  }

  // The controller route saves at once; it is used from the next connection.
  function route(key: 'adapter' | 'controller' | 'host', value: string, saved: string): void {
    if (value === saved) return;
    void run(() => api.setRoute({ [key]: value, expected: { [key]: saved } }), t("Saved · used from the next connection."));
  }

  function toggleTheme(): void {
    void run(() => settingsEdits.saveTheme(ui.theme === 'dark' ? 'light' : 'dark'));
  }
  function editControllerAddress(): void {
    osk.text(t("Controller address"), link.controller, (v) => route('controller', v, link.controller));
  }
  function editComputerAddress(): void {
    osk.text(t("This computer's address"), link.computer, (v) => route('host', v, link.computer));
  }
  function openPage(id: typeof ui.machinePage): void {
    ui.settingsQuery = '';
    ui.machinePage = id;
  }

  const linkText = $derived(connection.state === 'connected' ? connection.endpoint
    : connection.state === 'faulted' ? t("lost") : t("disconnected"));
  const identityText = $derived(machine.identity ? `${machine.identity.product_id} · ${machine.identity.program_version}` : '—');
  const scaleText = $derived(feedback ? `${displayNumber(feedback.scale, 'units/mm')} · ${feedback.cycle_us} µs` : '—');
  /** A soft-limit range, "from to to", on one axis. */
  const travel = (axis: 0 | 1): string =>
    bindings ? t("{0} to {1}", distance(bindings.extent[axis][0]), quantity(bindings.extent[axis][1], 'mm')) : '—';
  const jogSpeedsText = $derived(bindings
    ? `${displayNumber(bindings.jog_speed[0], 'mm/s')} · ${quantity(bindings.jog_speed[1], 'mm/s')}` : '—');
  const routeText = $derived(link.adapter ? `${link.adapter.description || link.adapter.name} · ${link.host ?? '—'}` : t("found on connection"));
  const attemptText = $derived(link.phase === 'failed' ? t("failed") : link.phase === 'idle' ? t("ended") : t("in progress"));

  const inputOn = (input: number): boolean => !!feedback && ((feedback.inputs >> (input - 1)) & 1) === 1;

  // Search shows every section; rows and groups that do not match hide.
  let testing = $state(false);
  let content = $state<HTMLElement | null>(null);
  let matches = $state(0);
  $effect(() => {
    void query;
    void ui.machinePage;
    if (!content) return;
    void tick().then(() => { if (content) matches = filterRows(content, query); });
  });

  function filterRows(root: HTMLElement, text: string): number {
    let shown = 0;
    for (const block of root.querySelectorAll<HTMLElement>('.search-block')) {
      const blockMatches = !!text && (block.dataset['keywords'] ?? '').toLocaleLowerCase().includes(text);
      const rows = [...block.querySelectorAll<HTMLElement>('.setting')];
      let any = false;
      for (const row of rows) {
        const group = row.closest<HTMLElement>('.setting-group');
        const title = group?.querySelector('h3')?.textContent?.toLocaleLowerCase() ?? '';
        const hit = !text || blockMatches || title.includes(text) || (row.textContent ?? '').toLocaleLowerCase().includes(text);
        row.hidden = !hit;
        any ||= hit;
      }
      for (const group of block.querySelectorAll<HTMLElement>('.setting-group')) {
        group.hidden = !!text && group.querySelectorAll('.setting').length > 0 && !group.querySelector('.setting:not([hidden])');
      }
      // Components without rows match on their whole text.
      const visible = !text || any || blockMatches || (!rows.length && (block.textContent ?? '').toLocaleLowerCase().includes(text));
      block.hidden = !visible;
      if (visible) shown += 1;
    }
    return shown;
  }
</script>

{#snippet group(name: string)}
  {#if name === 'display'}
    <div class="setting-group"><h3>{t("Display")}</h3>
      <div class="setting"><div class="lbl">{t("Units")}</div>
        <div class="seg" role="group" aria-label={t("Display units")}>
          <button class:on={units.system === 'metric'} aria-pressed={units.system === 'metric'} onclick={() => units.set('metric')}>{t("Metric · mm")}</button>
          <button class:on={units.system === 'imperial'} aria-pressed={units.system === 'imperial'} onclick={() => units.set('imperial')}>
            {t("Imperial · in")}
          </button>
        </div>
      </div>
      <div class="setting"><div class="lbl">Sprache · Language</div>
        <div class="seg" role="group" aria-label="Sprache · Language">
          <button class:on={locale === 'de'} aria-pressed={locale === 'de'} onclick={() => locale !== 'de' && chooseLocale('de')}>Deutsch</button>
          <button class:on={locale === 'en'} aria-pressed={locale === 'en'} onclick={() => locale !== 'en' && chooseLocale('en')}>English</button>
        </div>
      </div>
      <div class="setting">
        <div class="lbl">{t("Night mode")}<small>{t("Dark colours for this screen")}</small></div>
        <button
          class="switch"
          role="switch"
          class:on={ui.theme === 'dark'}
          aria-checked={ui.theme === 'dark'}
          aria-label={t("Night mode")}
          onclick={toggleTheme}
        ></button>
      </div>
      <div class="setting">
        <div class="lbl">{t("Hold to move or fire")}<small>{t("Every screen")}</small></div>
        <button class="val" data-numpad onclick={() => holdTime('move_ms', t('Hold to move or fire'))}>{holdSeconds(doc.hold.move_ms)}</button>
      </div>
      <div class="setting">
        <div class="lbl">{t("Hold to set origin")}<small>{t("Every screen")}</small></div>
        <button class="val" data-numpad onclick={() => holdTime('zero_ms', t('Hold to set origin'))}>{holdSeconds(doc.hold.zero_ms)}</button>
      </div>
    </div>
  {:else if name === 'checklists'}
    <div class="setting-group"><h3>{t("Checklists")}</h3>
      <div class="setting">
        <div class="lbl">{t("Before cutting")}<small>{t("Preflight · Fiber and CO₂")}</small></div>
        <button class="btn btn-ghost" onclick={() => (ui.checklistEditor = 'defaults')}>{t("Edit")}</button>
      </div>
      <div class="setting">
        <div class="lbl">{t("When paused")}<small>{t("Clear debris and check before resuming")}</small></div>
        <button class="btn btn-ghost" onclick={() => (ui.checklistEditor = 'pause')}>{t("Edit")}</button>
      </div>
      <div class="setting">
        <div class="lbl">{t("After the job")}<small>{t("Postflight · parking and part inspection")}</small></div>
        <button class="btn btn-ghost" onclick={() => (ui.checklistEditor = 'postflight')}>{t("Edit")}</button>
      </div>
    </div>
  {:else if name === 'interface'}
    <div class="setting-group"><h3>{t("This screen")}</h3>
      <div class="setting">
        <div class="lbl">{compact ? t("Full interface") : t("Phone interface")}<small>{t("The layout also follows the window size")}</small></div>
        <a class="btn btn-ghost" href={compact ? '/app' : '/mobile'} onclick={() => rememberLayout(compact ? 'full' : 'mobile')}>{t("Open")}</a>
      </div>
      <div class="setting"><div class="lbl">{t("Version")}</div><div class="val">{APP_VERSION}</div></div>
    </div>
  {:else if name === 'controller'}
    <div class="setting-group"><h3>{t("Controller")}</h3>
      <div class="setting"><div class="lbl">{t("Link")}<small>{t("UDP to the motion card")}</small></div><div class="val">{linkText}</div></div>
      <div class="setting"><div class="lbl">{t("Identity")}<small>{t("product · firmware")}</small></div><div class="val">{identityText}</div></div>
      <div class="setting">
        <div class="lbl">{t("Scale · cycle")}<small>{t("{0} · interpolation", t(unitLabel('units/mm')))}</small></div>
        <div class="val">{scaleText}</div>
      </div>
      <div class="setting">
        <div class="lbl">{t("Machine backup")}<small>{doc.files.backup?.name ?? doc.bindings_error ?? t('None loaded')}</small></div>
        <MachineFiles disabled={!!machine.operation} />
      </div>
    </div>
  {:else if name === 'laser'}
    <div class="setting-group"><h3>{t("Laser and head")}</h3>
      <div class="setting">
        <div class="lbl">{t("Operating mode")}<small>{t("Switch it in Machine tests")}</small></div>
        <div class="val">{laserLabel(doc.mode)}</div>
      </div>
      <div class="setting">
        <div class="lbl">{t("Applied on this connection")}</div>
        <div class="val">{machine.session.mode ? laserLabel(machine.session.mode) : t("not yet")}</div>
      </div>
      <div class="setting">
        <div class="lbl">{t("Head controller")}<small>{t("From the machine backup")}</small></div>
        <div class="val">{bindings ? (bindings.head_enabled ? t("configured") : t("none")) : '—'}</div>
      </div>
    </div>
  {:else if name === 'tests'}
    <div class="setting-group"><h3>{t("Machine tests")}</h3>
      <div class="setting"><div class="lbl">{t("Laser, gas and outputs")}<small>{t("Pulse the laser, test a gas valve, hold an output, switch Fiber / CO₂")}</small></div><button class="btn btn-ghost" onclick={() => (testing = true)}>{t("Open")}</button></div>
    </div>
  {:else if name === 'axes'}
    <div class="setting-group"><h3>{t("Axes and travel")}</h3>
      <div class="setting"><div class="lbl">{t("X travel")}<small>{t("soft limits")}</small></div><div class="val">{travel(0)}</div></div>
      <div class="setting"><div class="lbl">{t("Y travel")}<small>{t("soft limits")}</small></div><div class="val">{travel(1)}</div></div>
      <div class="setting"><div class="lbl">{t("Jog speeds")}<small>{t("slow · fast")}</small></div><div class="val">{jogSpeedsText}</div></div>
      <div class="setting">
        <div class="lbl">{t("Home")}<small>{t("on this connection")}</small></div>
        <div class="val">{machine.session.homed ? t("done") : t("needed")}</div>
      </div>
      <div class="setting">
        <div class="lbl">{t("Axis parameters")}<small>{t("Compared with the backup")}</small></div>
        <div class="val">{machine.session.parameters_verified ? t("checked") : t("not read")}</div>
      </div>
    </div>
  {:else if name === 'safety'}
    <div class="setting-group"><h3>{t("Safety inputs")}</h3>
      {#each bindings?.rules ?? [] as rule}
        <div class="setting">
          <div class="lbl">{rule.label}<small>{t("Input {0}", rule.input)}</small></div>
          <div class="val">{feedback ? (inputOn(rule.input) ? t("high") : t("low")) : '—'}</div>
        </div>
      {:else}
        <div class="setting"><div class="lbl">{t("No input rules")}<small>{doc.bindings_error ?? t('Load the machine backup and connect')}</small></div></div>
      {/each}
    </div>
  {:else if name === 'materials'}
    <div class="setting-group"><h3>{t("Material library")}</h3>
      <div class="setting">
        <div class="lbl">{t("Recipes")}<small>{t("{0} saved", doc.library.recipes.length)}</small></div>
        <button class="btn btn-ghost" onclick={() => (ui.tab = 'materials')}>{t("Open library")}</button>
      </div>
    </div>
  {:else if name === 'backup'}
    <div class="setting-group"><h3>{t("Backup")}</h3>
      <div class="setting">
        <div class="lbl">{t("Everything you made, in one ZIP")}<small>{t("Parts, jobs, recipes, sketches, sheets, settings and the machine backup. A copy is also kept daily in the data folder under backups (the last 14). To restore, unpack it over the data folder while OpenLaser is closed.")}</small></div>
        <a class="btn btn-ghost" href="/api/backup" download>{t("Download backup")}</a>
      </div>
    </div>
  {:else if name === 'process'}
    <MachineFiles kind="soft" disabled={!!machine.operation} />
  {:else if name === 'gas'}
    <GasCosts />
  {:else if name === 'head-calibration'}
    <div class="setting-group"><h3>{t("Head calibration")}</h3>
      <div class="setting">
        <div class="lbl">{t("On this connection")}<small>{t("Calibrate from Run")}</small></div>
        <div class="val">{machine.session.calibration ?? t('not yet')}</div>
      </div>
    </div>
  {:else if name === 'matrix'}
    <MatrixCalibration />
  {:else if name === 'scan'}
    <ScanCalibration />
  {:else if name === 'network'}
    <div class="setting-group"><h3>{t("Controller network")}</h3>
      <div class="setting"><div class="lbl">{t("Route")}<small>{t("Network adapter")}</small></div><div class="val">{routeText}</div></div>
      <div class="setting">
        <div class="lbl">{t("Last attempt")}<small>{attemptText}</small></div>
        <div class="val">{link.detail || (connection.state === 'connected' ? 'connected' : '—')}</div>
      </div>
      {#if link.choices.length}
        <div class="setting"><div class="lbl">{t("Machine adapter")}<small>{t("Used from the next connection")}</small></div>
          <div class="seg">
            {#each link.choices as choice}
              <button class:on={choice.name === link.remembered_adapter} onclick={() => route('adapter', choice.name, link.remembered_adapter)}>
                {choice.description || choice.name}
              </button>
            {/each}
          </div>
        </div>
      {/if}
      <div class="setting">
        <div class="lbl">{t("Controller address")}<small>{t("ip:port · used from the next connection")}</small></div>
        <button class="val" onclick={editControllerAddress}>{link.controller}</button>
      </div>
      <div class="setting">
        <div class="lbl">{t("This computer's address")}<small>{t("ip/prefix · used from the next connection")}</small></div>
        <button class="val" onclick={editComputerAddress}>{link.computer}</button>
      </div>
    </div>
  {:else if name === 'phones'}
    <div class="setting-group"><h3>{t("Phones and tablets")}</h3>
      <div class="setting">
        <div class="lbl">{t("Local network")}<small>{access.info?.address.replace('http://', '')}</small></div>
        <button class="btn btn-ghost" onclick={() => (access.manage = true)}>{t("Open")}</button>
      </div>
    </div>
  {:else if name === 'xml'}
    <MachineSettings search={ui.settingsQuery} />
  {/if}
{/snippet}

<section class="panel main settings-page" class:section-only={sectionOnly}>
  {#if !sectionOnly}
    <nav class="settings-sidebar" aria-label={t("Settings sections")}>
      <div class="settings-title"><h1>{t("Settings")}</h1><span>v{APP_VERSION}</span></div>
      <label class="search settings-search"><i class="ic ic-search"></i><input
        type="search"
        aria-label={t("Search settings")}
        placeholder={t("Search…")}
        bind:value={ui.settingsQuery}
      /></label>
      {#each SETTINGS_PAGES as item (item.id)}
        <button
          class="settings-link"
          class:on={!query && ui.machinePage === item.id}
          aria-current={!query && ui.machinePage === item.id ? 'page' : undefined}
          onclick={() => openPage(item.id)}
        >{t(item.label)}</button>
      {/each}
    </nav>
  {/if}
  <div class="settings-content" bind:this={content}>
    {#if query}
      {#if !sectionOnly}<h2 class="settings-heading">{t("Results for “{0}”", ui.settingsQuery.trim())}</h2>{/if}
      {#each SETTINGS_PAGES as item (item.id)}
        {#each item.groups as name (name)}
          <div class="search-block" data-keywords="{t(item.label)} {item.keywords ?? ''}"><p class="search-section">{t(item.label)}</p>{@render group(name)}</div>
        {/each}
      {/each}
      {#if matches === 0}<p class="muted">{t("Nothing matches. Try a shorter word.")}</p>{/if}
    {:else}
      {#if !sectionOnly}<h2 class="settings-heading">{t(page.label)}</h2>{/if}
      <div class="settings-groups" class:wide-page={page.wide}>
        {#each page.groups as name (name)}<div class="search-block">{@render group(name)}</div>{/each}
      </div>
    {/if}
  </div>
</section>

{#if testing}<MachineTools onclose={() => (testing = false)} />{/if}
{#if ui.checklistEditor}<PreflightEditor scope={ui.checklistEditor} onclose={() => (ui.checklistEditor = null)} />{/if}

<style>
  .settings-page { grid-column: 1 / -1; display: grid; grid-template-columns: 250px minmax(0, 1fr); gap: 0; padding: 0; overflow: hidden; }
  .settings-page.section-only { grid-template-columns: minmax(0, 1fr); border: 0; background: transparent; box-shadow: none; }
  .section-only .settings-content { padding: 0; overflow: visible; }
  .settings-sidebar { display: flex; flex-direction: column; gap: 4px; padding: 16px 12px; border-right: 1px solid var(--line); overflow-y: auto; }
  .settings-title { display: flex; align-items: baseline; flex-wrap: wrap; gap: 8px; padding: 0 8px 8px; }
  .settings-title span { font-size: var(--t-sm); color: var(--ink-3); }
  .settings-search :global(input) { min-width: 0; width: 100%; }
  .settings-sidebar { min-width: 0; overflow-x: hidden; }
  .settings-link { hyphens: auto; overflow-wrap: normal; height: auto; padding-top: 10px; padding-bottom: 10px; }
  .settings-search { flex: none; min-width: 0; margin: 0 0 8px; }
  .settings-link { min-height: 48px; padding: 0 12px; border: 0; border-radius: 10px; background: transparent; color: var(--ink-2); font: inherit; font-weight: 600; text-align: left; cursor: pointer; }
  .settings-link.on { background: var(--accent-soft); color: var(--accent-2); }
  .settings-content { min-width: 0; overflow-y: auto; padding: 16px 22px 22px; display: flex; flex-direction: column; gap: 12px; }
  .settings-heading { margin: 4px 0; }
  .settings-groups { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(340px, 100%), 1fr)); gap: 12px; align-items: start; }
  .settings-groups.wide-page { grid-template-columns: minmax(0, 1fr); }
  .search-section { margin: 8px 0 6px; font-size: var(--t-sm); font-weight: 700; color: var(--ink-3); text-transform: uppercase; letter-spacing: .06em; }
  @media (max-width: 900px) { .settings-page { grid-template-columns: 200px minmax(0, 1fr); } }
</style>
