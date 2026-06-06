<script>
  import { createEventDispatcher } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  export let isOpen = false;
  /** @type {{ manager: string, name: string, display_name: string, version?: string | null, category: string } | null} */
  export let packageInfo = null;
  /** @type {{ label: string, supports_force_uninstall: boolean, supports_cleanup_orphans: boolean } | null} */
  export let managerStatus = null;
  /** @type {'update' | 'uninstall'} */
  export let operation = 'update';

  const dispatch = createEventDispatcher();

  let force = false;
  let cleanupOrphans = false;
  let confirmationText = '';
  let isRunning = false;
  /** @type {{ success: boolean, message: string, details?: string | null, command?: string | null, dry_run: boolean } | null} */
  let result = null;

  $: packageName = packageInfo?.name || '';
  $: managerLabel = managerStatus?.label || packageInfo?.manager || 'Manager';
  $: isUninstall = operation === 'uninstall';
  $: canForce = Boolean(managerStatus?.supports_force_uninstall) && isUninstall;
  $: canCleanup = Boolean(managerStatus?.supports_cleanup_orphans) && isUninstall && !force;
  $: destructiveReady = !isUninstall || confirmationText === packageName;

  $: if (!isOpen) {
    resetState();
  }

  $: if (force) {
    cleanupOrphans = false;
  }

  function resetState() {
    force = false;
    cleanupOrphans = false;
    confirmationText = '';
    isRunning = false;
    result = null;
  }

  function closeModal() {
    if (isRunning) return;
    dispatch('close');
  }

  /** @param {boolean} dryRun */
  async function runOperation(dryRun) {
    if (!packageInfo) return;
    if (!dryRun && !destructiveReady) {
      result = {
        success: false,
        message: `Type "${packageName}" to confirm uninstall.`,
        details: null,
        dry_run: false
      };
      return;
    }

    isRunning = true;
    result = null;

    try {
      result = /** @type {{ success: boolean, message: string, details?: string | null, command?: string | null, dry_run: boolean }} */ (await invoke('execute_package_operation', {
        args: {
          manager: packageInfo.manager,
          package_name: packageName,
          operation,
          dry_run: dryRun,
          force,
          cleanup_orphans: canCleanup ? cleanupOrphans : false
        }
      }));

      if (result?.success && !dryRun) {
        dispatch('completed', { manager: packageInfo.manager, operation, packageName });
      }
    } catch (error) {
      result = {
        success: false,
        message: String(error),
        details: String(error),
        dry_run: dryRun
      };
    } finally {
      isRunning = false;
    }
  }
</script>

{#if isOpen && packageInfo}
  <div
    class="modal-backdrop"
    on:click={closeModal}
    on:keydown={(event) => event.key === 'Escape' && closeModal()}
    tabindex="0"
    role="button"
    aria-label="Close operation dialog"
  >
    <div
      class="modal-content"
      on:click|stopPropagation
      on:keydown|stopPropagation
      role="dialog"
      aria-modal="true"
      aria-labelledby="operation-title"
      tabindex="-1"
    >
      <header class="modal-header">
        <div>
          <p>{managerLabel}</p>
          <h2 id="operation-title">
            {isUninstall ? 'Uninstall' : 'Update'} {packageInfo.display_name || packageName}
          </h2>
        </div>
        <button class="icon-button" on:click={closeModal} disabled={isRunning} aria-label="Close">x</button>
      </header>

      <div class="package-summary">
        <span>{packageName}</span>
        {#if packageInfo.version}
          <span>{packageInfo.version}</span>
        {/if}
        <span>{packageInfo.category}</span>
      </div>

      {#if isUninstall}
        <div class="options-panel">
          <label class="checkbox-row">
            <input type="checkbox" bind:checked={force} disabled={!canForce || isRunning} />
            <span>
              Force uninstall
              <small>{canForce ? 'Ignore dependency protections. Use only as a last resort.' : 'Not supported for this manager.'}</small>
            </span>
          </label>

          <label class="checkbox-row">
            <input type="checkbox" bind:checked={cleanupOrphans} disabled={!canCleanup || isRunning} />
            <span>
              Remove orphaned packages after uninstall
              <small>{canCleanup ? 'Runs autoremove only after a successful safe uninstall.' : 'Unavailable for this operation.'}</small>
            </span>
          </label>

          <label class="confirm-field">
            Type package name to enable actual uninstall
            <input
              type="text"
              bind:value={confirmationText}
              disabled={isRunning}
              placeholder={packageName}
              autocomplete="off"
            />
          </label>
        </div>
      {/if}

      {#if result}
        <div class:success={result.success} class:error={!result.success} class="operation-result" role="status">
          <strong>{result.success ? 'Completed' : 'Failed'}</strong>
          <p>{result.message}</p>
          {#if result.command}
            <code>{result.command}</code>
          {/if}
          {#if result.details}
            <details>
              <summary>Command output</summary>
              <pre>{result.details}</pre>
            </details>
          {/if}
        </div>
      {/if}

      <footer class="modal-actions">
        <button class="secondary-button" on:click={() => runOperation(true)} disabled={isRunning}>
          {isRunning ? 'Running...' : 'Preview'}
        </button>
        <button
          class:is-danger={isUninstall}
          class="primary-button"
          on:click={() => runOperation(false)}
          disabled={isRunning || !destructiveReady}
        >
          {isRunning ? 'Running...' : isUninstall ? 'Uninstall' : 'Update'}
        </button>
      </footer>
    </div>
  </div>
{/if}
