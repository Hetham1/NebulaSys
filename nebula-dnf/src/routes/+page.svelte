<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';

  /**
   * @typedef {Object} DisplayablePackage // Renamed from PackageInfo
   * @property {string} name
   */

  /**
   * @typedef {Object} UserPackageWithDependencies
   * @property {string} name
   * @property {DisplayablePackage[]} dependencies
   * @property {boolean} [showDependencies] // For UI state, make it optional for initial type
   */

  /** @type {(UserPackageWithDependencies[] | DisplayablePackage[])} */
  let packages = [];
  let errorMessage = '';
  let isLoading = true;
  /** @type {'all' | 'user'} */
  let packageViewMode = 'user'; // Default to user-installed packages

  /** @param {'all' | 'user'} mode */
  async function fetchPackages(mode) {
    isLoading = true;
    errorMessage = '';
    packages = []; 
    try {
      let result;
      if (mode === 'user') {
        result = await invoke('list_user_installed_packages');
        // Initialize showDependencies state for user packages
        packages = (/** @type {UserPackageWithDependencies[]} */ (result)).map(pkg => ({ 
          ...pkg, 
          showDependencies: false 
        }));
      } else {
        result = await invoke('list_installed_packages');
        packages = result; // This will be DisplayablePackage[]
      }
    } catch (error) {
      console.error(`Error loading ${mode} packages:`, error);
      errorMessage = String(error);
    }
    isLoading = false;
  }

  /** @param {'all' | 'user'} mode */
  function setViewMode(mode) {
    packageViewMode = mode;
    fetchPackages(mode);
  }

  /** @param {string} packageName // Pass name to identify package to toggle */
  function toggleDependencies(packageName) {
    packages = packages.map(pkg => {
      // Check if the package is a UserPackageWithDependencies and matches the name
      if ('dependencies' in pkg && pkg.name === packageName) {
        const userPkg = /** @type {UserPackageWithDependencies} */ (pkg);
        return { ...userPkg, showDependencies: !userPkg.showDependencies };
      }
      return pkg;
    });
}

  onMount(() => {
    fetchPackages(packageViewMode);
  });
</script>

<div style="padding: 20px; font-family: sans-serif;">
  <h1>DNF Package Manager</h1>

  <div style="margin-bottom: 15px;">
    <button on:click={() => setViewMode('user')} disabled={isLoading || packageViewMode === 'user'}>
      Show User Installed (with Dependencies)
    </button>
    <button on:click={() => setViewMode('all')} disabled={isLoading || packageViewMode === 'all'} style="margin-left: 10px;">
      Show All Installed (Flat List)
    </button>
  </div>

  {#if errorMessage}
    <p style="color: red;">Error: {errorMessage}</p>
  {/if}

  {#if isLoading}
    <p>Loading {packageViewMode === 'user' ? 'user installed' : 'all'} packages...</p>
  {:else if packages.length > 0}
    <h2>
      {packageViewMode === 'user' ? 'User Installed' : 'All Installed'} Packages ({packages.length}):
    </h2>
    <ul style="list-style-type: none; padding-left: 0;">
      {#each packages as pkg (pkg.name)} 
        {#if packageViewMode === 'user'}
          {@const userPkg = /** @type {UserPackageWithDependencies} */ (pkg)}
          <li style="margin-bottom: 5px; border: 1px solid #eee; padding: 5px; border-radius: 4px;">
            <div style="cursor: pointer; padding: 4px; background-color: #f9f9f9; border-radius: 3px;" on:click={() => toggleDependencies(userPkg.name)} role="button" tabindex="0" on:keydown={(e) => e.key === 'Enter' && toggleDependencies(userPkg.name)}>
              <strong>{userPkg.name}</strong> 
              <span style="font-size: 0.9em; color: #555;">
                ({userPkg.showDependencies ? 'Hide' : 'Show'} {userPkg.dependencies.length} Dependencies)
              </span>
            </div>
            {#if userPkg.showDependencies && userPkg.dependencies.length > 0}
              <ul style="margin-top: 8px; padding-left: 25px; list-style-type: disc; background-color: #fff; border-top: 1px dashed #ddd; padding-top: 5px;">
                {#each userPkg.dependencies as dep (dep.name)}
                  <li style="padding: 1px 0;">{dep.name}</li>
                {/each}
              </ul>
            {:else if userPkg.showDependencies && userPkg.dependencies.length === 0}
               <p style="padding-left: 25px; font-style: italic; margin-top: 5px; color: #777;">No dependencies listed for this package.</p>
            {/if}
          </li>
        {:else}
          {@const displayPkg = /** @type {DisplayablePackage} */ (pkg)}
          <li style="padding: 4px 2px; border-bottom: 1px dotted #eee;">{displayPkg.name}</li>
        {/if}
      {/each}
    </ul>
  {:else if !errorMessage}
    <p>No {packageViewMode === 'user' ? 'user installed' : 'all'} packages found.</p>
  {/if}
</div>

<svelte:head>
  <title>Nebula DNF Manager</title>
</svelte:head>
