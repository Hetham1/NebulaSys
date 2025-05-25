<!-- UninstallModal.svelte -->
<script>
  import { createEventDispatcher } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  export let isOpen = false;
  export let packageName = '';

  const dispatch = createEventDispatcher();

  /**
   * @typedef {Object} PackageOperationResultType
   * @property {boolean} success
   * @property {string} message
   * @property {string | null | undefined} [details]
   */

  const UninstallMode = {
    SAFE: 'Safe',
    FORCE: 'Force',
    DRY_RUN_SAFE: 'DryRunSafe',
    DRY_RUN_FORCE: 'DryRunForce'
  };

  let selectedMode = UninstallMode.SAFE; // 'Safe' or 'Force' for actual uninstall
  let cleanupOrphans = false;
  let isLoading = false;
  /** @type {PackageOperationResultType | null} */
  let operationResult = null; // { success: boolean, message: string, details: string | null }
  let dryRunOutput = '';

  async function performOperation(isDryRun = false) {
    isLoading = true;
    operationResult = null;
    dryRunOutput = '';

    let modeForBackend;
    if (isDryRun) {
      modeForBackend = selectedMode === UninstallMode.SAFE ? UninstallMode.DRY_RUN_SAFE : UninstallMode.DRY_RUN_FORCE;
    } else {
      modeForBackend = selectedMode;
    }

    const uninstallParameters = {
      package_name: packageName,
      mode: modeForBackend,
      cleanup_orphans: (modeForBackend === UninstallMode.SAFE || modeForBackend === UninstallMode.DRY_RUN_SAFE) ? cleanupOrphans : false,
    };

    try {
      // The backend expects all parameters nested under a single 'args' key.
      const result = /** @type {PackageOperationResultType} */ (await invoke('execute_package_uninstall', { args: uninstallParameters }));
      if (isDryRun) {
        dryRunOutput = result.details || 'No specific details from dry run.';
        operationResult = { success: true, message: result.message, details: null };
      } else {
        operationResult = result;
        if (result.success) {
          setTimeout(() => dispatch('uninstallCompleted'), 500); // Give time to read message before closing
        }
      }
    } catch (error) {
      operationResult = {
        success: false,
        message: `Failed to invoke uninstall command: ${error}`,
        details: String(error),
      };
    }
    isLoading = false;
  }

  function closeModal() {
    if (isLoading) return;
    dispatch('close');
    // Reset state for next open
    operationResult = null;
    dryRunOutput = '';
    cleanupOrphans = false;
    selectedMode = UninstallMode.SAFE;
  }

  // Reset orphan checkbox if force is selected
  $: if (selectedMode === UninstallMode.FORCE) {
    cleanupOrphans = false;
  }
</script>

{#if isOpen}
  <div 
    class="modal-backdrop"
    style="position: fixed !important; top: 0 !important; left: 0 !important; width: 100vw !important; height: 100vh !important; z-index: 9999 !important; background-color: rgba(0,0,0,0.75) !important; display: flex !important; justify-content: center !important; align-items: center !important;"
    on:click={closeModal} 
    on:keydown={(e) => e.key === 'Escape' && closeModal()} 
    tabindex="0" 
    role="button" 
    aria-label="Close modal">
    <div class="modal-content" on:click|stopPropagation on:keydown|stopPropagation role="dialog" aria-modal="true" aria-labelledby="modal-title" tabindex="-1">
      <h2 id="modal-title">Uninstall Options: {packageName}</h2>
      
      <div class="options-group">
        <label class="option-label" for="uninstall-safe-radio">
          <input id="uninstall-safe-radio" type="radio" bind:group={selectedMode} value={UninstallMode.SAFE} name="uninstall-type" />
          Safe Uninstall (Recommended)
        </label>
        <p class="option-description">Removes only '<strong>{packageName}</strong>'. Use 'dnf remove'.</p>
        
        {#if selectedMode === UninstallMode.SAFE}
        <label class="checkbox-label sub-option">
          <input type="checkbox" bind:checked={cleanupOrphans} />
          Also remove unused dependencies (orphans) after uninstalling '<strong>{packageName}</strong>'. (Uses 'dnf autoremove')
        </label>
        {/if}

        <label class="option-label">
          <input type="radio" bind:group={selectedMode} value={UninstallMode.FORCE} name="uninstall-type" />
          Force Uninstall (Dangerous)
        </label>
        <p class="option-description">Removes '<strong>{packageName}</strong>' ignoring dependencies. May break your system. Uses 'rpm -e --nodeps'.</p>
      </div>

      {#if dryRunOutput}
        <div class="dry-run-output">
          <strong>Dry Run Output:</strong>
          <pre>{dryRunOutput}</pre>
        </div>
      {/if}

      {#if operationResult && !dryRunOutput && operationResult.message} <!-- Don't show if dry run output is already shown for this op -->
        <div class="operation-status {operationResult.success ? 'success' : 'error'}" role="alert">
          <p><strong>{operationResult.success ? 'Success' : 'Error'}:</strong> {operationResult.message}</p>
          {#if operationResult.details && !operationResult.success}
            <pre class="error-details">{operationResult.details}</pre>
          {/if}
        </div>
      {/if}

      <div class="modal-actions">
        <button class="btn-secondary" on:click={() => performOperation(true)} disabled={isLoading}>
          {#if isLoading && (selectedMode === UninstallMode.DRY_RUN_SAFE || selectedMode === UninstallMode.DRY_RUN_FORCE) }Previewing...{:else}Preview Changes (Dry Run){/if}
        </button>
        <button class="btn-danger" on:click={() => performOperation(false)} disabled={isLoading}>
          {#if isLoading && !(selectedMode === UninstallMode.DRY_RUN_SAFE || selectedMode === UninstallMode.DRY_RUN_FORCE) }Uninstalling...{:else}Uninstall {packageName}{/if}
        </button>
        <button class="btn-neutral" on:click={closeModal} disabled={isLoading}>Cancel</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background-color: rgba(0, 0, 0, 0.7);
    display: flex;
    justify-content: center;
    align-items: center;
    z-index: 1000;
  }
  .modal-content {
    background-color: var(--nebula-surface-light, #2c2c4f);
    padding: 25px;
    border-radius: 10px;
    border: 1px solid var(--nebula-border, #3c3c6c);
    box-shadow: 0 5px 20px rgba(0,0,0,0.5);
    width: 90%;
    max-width: 600px;
    color: var(--nebula-text-primary, #e0e0ff);
  }
  h2 {
    margin-top: 0;
    margin-bottom: 20px;
    color: var(--nebula-accent, #ff00aa);
    font-size: 1.6em;
  }
  .options-group {
    margin-bottom: 20px;
  }
  .option-label {
    display: block;
    margin-bottom: 5px;
    font-weight: bold;
    cursor: pointer;
  }
  .option-label input[type="radio"] {
    margin-right: 8px;
    vertical-align: middle;
  }
  .option-description {
    font-size: 0.9em;
    color: var(--nebula-text-secondary, #a0a0cc);
    margin-top: 0;
    margin-left: 25px; /* Indent description */
    margin-bottom: 15px;
  }
  .checkbox-label {
    display: block;
    font-size: 0.9em;
    margin-left: 25px; /* Indent sub-option */
    margin-bottom: 10px;
    color: var(--nebula-text-secondary, #a0a0cc);
    cursor: pointer;
  }
  .checkbox-label input[type="checkbox"] {
    margin-right: 8px;
    vertical-align: middle;
  }
  .dry-run-output, .operation-status {
    margin-top: 15px;
    padding: 10px;
    border-radius: 5px;
    max-height: 200px;
    overflow-y: auto;
    font-size: 0.9em;
  }
  .dry-run-output strong, .operation-status strong {
      display: block;
      margin-bottom: 5px;
  }
  .dry-run-output pre, .operation-status pre {
    white-space: pre-wrap;
    word-break: break-all;
    background-color: var(--nebula-surface, #1a1a3a);
    padding: 8px;
    border-radius: 4px;
  }
  .operation-status.success {
    background-color: rgba(0, 255, 170, 0.1);
    border: 1px solid var(--nebula-green-glow, #00ffaa);
    color: var(--nebula-green-glow, #00ffaa);
  }
  .operation-status.error {
    background-color: rgba(255, 85, 85, 0.1);
    border: 1px solid var(--nebula-red-glow, #ff5555);
    color: var(--nebula-red-glow, #ff5555);
  }
  .error-details {
      color: var(--nebula-text-secondary); /* Dimmer for stack trace like details */
  }
  .modal-actions {
    margin-top: 25px;
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  /* Re-use button styles from +page.svelte if possible, or define themed ones here */
  .modal-actions button {
    padding: 10px 18px;
    border: none;
    border-radius: 20px;
    font-weight: bold;
    cursor: pointer;
    transition: background-color 0.3s ease, transform 0.2s ease;
  }
  .modal-actions button:disabled {
      opacity: 0.6;
      cursor: not-allowed;
  }
  .btn-secondary {
    background-color: var(--nebula-border, #3c3c6c);
    color: var(--nebula-text-primary, #e0e0ff);
  }
  .btn-secondary:hover:not(:disabled) {
    background-color: #4c4c7c;
  }
  .btn-danger {
    background-color: var(--nebula-red-glow, #ff5555);
    color: black;
  }
  .btn-danger:hover:not(:disabled) {
    background-color: #ff7777;
  }
  .btn-neutral {
    background-color: var(--nebula-surface, #1a1a3a);
    color: var(--nebula-text-secondary, #a0a0cc);
    border: 1px solid var(--nebula-border, #3c3c6c);
  }
  .btn-neutral:hover:not(:disabled) {
    background-color: var(--nebula-border, #3c3c6c);
    color: var(--nebula-text-primary, #e0e0ff);
  }
</style> 