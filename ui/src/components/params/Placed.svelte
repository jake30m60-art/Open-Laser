<script lang="ts">
  import { t } from '../../i18n/index';

  // How many places were picked on the drawing, with Clear and Pick.
  import { plural } from '../../lib/format';

  let { count, what, disabled = false, onclear, onpick }: {
    count: number;
    /** What one place is, such as "joint". */
    what: string;
    disabled?: boolean;
    onclear: () => void;
    onpick: () => void;
  } = $props();
</script>

<div class="param"><div class="lbl">{plural(count, what)} {t("on the drawing")}</div>
  <div class="row">
    <button class="btn btn-ghost" onclick={onclear} disabled={disabled || count === 0}>{t("Clear")}</button>
    <button class="btn btn-soft" onclick={onpick} {disabled}>{t("Pick")}</button>
  </div>
</div>
