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

  /** @type {Map<'all' | 'user', (UserPackageWithDependencies[] | DisplayablePackage[])>} */
  let packageCache = new Map();

  /** @param {'all' | 'user'} mode */
  async function fetchPackages(mode, forceRefresh = false) {
    isLoading = true;
    errorMessage = '';
    // packages = []; // Clear only if we are actually fetching

    if (!forceRefresh && packageCache.has(mode)) {
      const cachedPackages = packageCache.get(mode);
      if (cachedPackages) {
        packages = cachedPackages;
        isLoading = false;
        console.log(`Loaded ${mode} packages from cache.`);
        return;
      }
    }
    
    console.log(`Fetching ${mode} packages from backend...`);
    packages = []; // Clear previous packages before new fetch

    try {
      let result;
      if (mode === 'user') {
        result = await invoke('list_user_installed_packages', { forceRefresh });
        const userPackages = (/** @type {UserPackageWithDependencies[]} */ (result)).map(pkg => ({ 
          ...pkg, 
          showDependencies: false 
        }));
        packages = userPackages;
        packageCache.set(mode, userPackages);
      } else {
        result = await invoke('list_installed_packages');
        packages = result; // This will be DisplayablePackage[]
        packageCache.set(mode, result);
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
    fetchPackages(mode); // Will use cache by default
  }

  function refreshCurrentView() {
    console.log('Forcing refresh for current view:', packageViewMode);
    fetchPackages(packageViewMode, true); // Pass true to force refresh
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

<style>
  :root {
    --nebula-bg: linear-gradient(145deg, #101028, #281838);
    --nebula-surface: #1a1a3a;
    --nebula-surface-light: #2c2c4f;
    --nebula-text-primary: #e0e0ff;
    --nebula-text-secondary: #a0a0cc;
    --nebula-accent: #ff00aa; /* Vibrant magenta */
    --nebula-accent-hover: #ff40bf;
    --nebula-border: #3c3c6c;
    --nebula-green-glow: #00ffaa;
    --nebula-red-glow: #ff5555;
    --font-main: 'Segoe UI', 'Roboto', 'Helvetica Neue', Arial, sans-serif;
  }

  .nebula-container {
    min-height: 100vh;
    background: var(--nebula-bg);
    color: var(--nebula-text-primary);
    font-family: var(--font-main);
    padding: 25px;
    box-sizing: border-box;
  }

  h1 {
    text-align: center;
    font-size: 2.8em;
    margin-bottom: 30px;
    color: var(--nebula-text-primary);
    text-shadow: 0 0 10px var(--nebula-accent), 0 0 20px var(--nebula-accent);
  }

  h2 {
    color: var(--nebula-text-secondary);
    border-bottom: 1px solid var(--nebula-border);
    padding-bottom: 10px;
    margin-top: 30px;
    font-size: 1.6em;
  }

  .button-group {
    display: flex;
    justify-content: center;
    gap: 15px;
    margin-bottom: 30px;
  }

  button {
    background-color: var(--nebula-accent);
    color: var(--nebula-text-primary);
    border: none;
    padding: 12px 22px;
    border-radius: 25px;
    cursor: pointer;
    font-size: 1em;
    font-weight: bold;
    transition: background-color 0.3s ease, transform 0.2s ease, box-shadow 0.3s ease;
    box-shadow: 0 0 8px transparent;
  }

  button:hover:not(:disabled) {
    background-color: var(--nebula-accent-hover);
    transform: translateY(-2px);
    box-shadow: 0 0 15px var(--nebula-accent-hover);
  }

  button:disabled {
    background-color: var(--nebula-border);
    color: var(--nebula-text-secondary);
    cursor: not-allowed;
    opacity: 0.7;
  }

  .loading-message,
  .no-packages-message {
    text-align: center;
    font-size: 1.2em;
    color: var(--nebula-text-secondary);
    margin-top: 40px;
    padding: 20px;
    background-color: var(--nebula-surface);
    border-radius: 8px;
    border: 1px solid var(--nebula-border);
  }

  .error-message {
    text-align: center;
    font-size: 1.1em;
    color: var(--nebula-red-glow);
    margin-top: 20px;
    padding: 15px;
    background-color: rgba(255, 85, 85, 0.1);
    border: 1px solid var(--nebula-red-glow);
    border-radius: 8px;
  }

  .packages-list {
    list-style-type: none;
    padding-left: 0;
  }

  .package-item,
  .package-item-flat {
    background-color: var(--nebula-surface);
    margin-bottom: 10px;
    border: 1px solid var(--nebula-border);
    border-radius: 8px;
    padding: 15px;
    transition: box-shadow 0.3s ease, transform 0.2s ease;
  }
  
  .package-item:hover {
      box-shadow: 0 0 10px var(--nebula-accent);
      /* transform: scale(1.01); */
  }

  .package-header {
    cursor: pointer;
    padding: 8px 0;
    /* background-color: var(--nebula-surface-light); */
    border-radius: 5px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .package-header strong {
    font-size: 1.2em;
    color: var(--nebula-text-primary);
  }

  .package-header .dep-count {
    font-size: 0.9em;
    color: var(--nebula-accent);
    background-color: rgba(255, 0, 170, 0.1);
    padding: 3px 8px;
    border-radius: 10px;
  }

  .dependencies-list {
    margin-top: 12px;
    padding-left: 30px;
    list-style-type: none; /* Using custom bullets */
    /* background-color: var(--nebula-surface-light); */
    border-top: 1px dashed var(--nebula-border);
    padding-top: 12px;
  }

  .dependencies-list li {
    padding: 4px 0;
    color: var(--nebula-text-secondary);
    position: relative;
  }
  .dependencies-list li::before {
    content: '◆'; /* Diamond bullet */
    color: var(--nebula-green-glow);
    position: absolute;
    left: -20px;
    font-size: 0.8em;
  }

  .no-deps-message {
    padding-left: 30px;
    font-style: italic;
    margin-top: 10px;
    color: var(--nebula-text-secondary);
  }

  .package-item-flat {
    padding: 10px 15px;
    color: var(--nebula-text-secondary);
  }
</style>

<div class="nebula-container">
  <h1>Nebula DNF Explorer</h1>

  <div class="button-group">
    <button on:click={() => setViewMode('user')} disabled={isLoading || packageViewMode === 'user'}>
      User Packages
    </button>
    <button on:click={() => setViewMode('all')} disabled={isLoading || packageViewMode === 'all'}>
      All Packages
    </button>
    <button on:click={refreshCurrentView} disabled={isLoading}>
      Refresh View
    </button>
  </div>

  {#if errorMessage}
    <p class="error-message">Error: {errorMessage}</p>
  {/if}

  {#if isLoading}
    <p class="loading-message">Summoning data from the DNF void for {packageViewMode === 'user' ? 'user installed' : 'all'} packages...</p>
  {:else if packages.length > 0}
    <h2>
      {packageViewMode === 'user' ? 'User Installed' : 'All Installed'} Packages ({packages.length}):
    </h2>
    <ul class="packages-list">
      {#each packages as pkg (pkg.name)} 
        {#if packageViewMode === 'user'}
          {@const userPkg = /** @type {UserPackageWithDependencies} */ (pkg)}
          <li class="package-item">
            <div class="package-header" on:click={() => toggleDependencies(userPkg.name)} role="button" tabindex="0" on:keydown={(e) => e.key === 'Enter' && toggleDependencies(userPkg.name)}>
              <strong>{userPkg.name}</strong> 
              <span class="dep-count">
                {userPkg.showDependencies ? 'Hide' : 'Show'} {userPkg.dependencies.length} Dependencies
              </span>
            </div>
            {#if userPkg.showDependencies && userPkg.dependencies.length > 0}
              <ul class="dependencies-list">
                {#each userPkg.dependencies as dep (dep.name)}
                  <li>{dep.name}</li>
                {/each}
              </ul>
            {:else if userPkg.showDependencies && userPkg.dependencies.length === 0}
               <p class="no-deps-message">No known dependencies in this dimension.</p>
            {/if}
          </li>
        {:else}
          {@const displayPkg = /** @type {DisplayablePackage} */ (pkg)}
          <li class="package-item-flat">{displayPkg.name}</li>
        {/if}
      {/each}
    </ul>
  {:else if !errorMessage}
    <p class="no-packages-message">The DNF void seems empty for {packageViewMode === 'user' ? 'user installed' : 'all'} packages.</p>
  {/if}
</div>

<svelte:head>
  <title>Nebula DNF Explorer</title>
</svelte:head>
