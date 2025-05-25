<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import UninstallModal from './UninstallModal.svelte'; // Import the modal

  /** @type {Record<string, string>} */
  const PackageCategory = {
    MANUAL: 'Manual',
    DESKTOP_ENVIRONMENT: 'DesktopEnvironment',
    SYSTEM: 'System',
    LIBRARY: 'Library',
    DEVELOPMENT: 'Development',
    MULTIMEDIA: 'Multimedia',
    OFFICE: 'Office',
    GAMES: 'Games',
    UTILITY: 'Utility',
    NETWORK: 'Network',
    SECURITY: 'Security',
    OTHER_APPLICATION: 'OtherApplication',
    UNKNOWN: 'Unknown',
    ALL: 'All Categories' // Special value for filter UI
  };

  /**
   * @typedef {Object} DisplayablePackage
   * @property {string} name
   */

  /**
   * @typedef {Object} UserPackageWithDependencies
   * @property {string} name
   * @property {string} category // Mirrors PackageCategory enum from Rust
   * @property {DisplayablePackage[]} dependencies
   * @property {boolean} [showDependencies]
   */

  /**
   * @typedef {Object} PackageOperationResultType
   * @property {boolean} success
   * @property {string} message
   * @property {string | null | undefined} [details]
   */

  /** @type {(UserPackageWithDependencies[] | DisplayablePackage[])} */
  let packages = [];
  let errorMessage = '';
  let isLoading = true;
  /** @type {'all' | 'user'} */
  let packageViewMode = 'user';
  let searchTerm = '';
  let selectedCategoryFilter = PackageCategory.ALL;

  /** @type {Array<{key: string, value: string}>} */
  let availableCategoriesForFilter = [{ key: 'ALL', value: PackageCategory.ALL }]; // Initialize with ALL

  /** @type {Map<'all' | 'user', (UserPackageWithDependencies[] | DisplayablePackage[])>} */
  let packageCache = new Map();

  /** @type {Record<string, {isLoading: boolean, message: string, isError: boolean, details?: string | null}>} */
  let packageOpStatus = {}; // To track status of ops per package, e.g. { "packageName": { isLoading: true, message: "", isError: false } }
  let activeOperationCount = 0; // To disable global refresh/view change during any package operation

  // Uninstall Modal State
  let isUninstallModalOpen = false;
  let packageForUninstall = '';

  /** @param {'all' | 'user'} mode */
  async function fetchPackages(mode, forceRefresh = false) {
    if (activeOperationCount > 0 && !forceRefresh) {
        errorMessage = "Please wait for ongoing package operations to complete before fetching.";
        return;
    }
    isLoading = true;
    errorMessage = '';

    if (!forceRefresh && packageCache.has(mode)) {
      const cachedPackages = packageCache.get(mode);
      if (cachedPackages) {
        packages = cachedPackages;
        if (mode === 'user') {
          updateAvailableCategoriesAndSelection(/** @type {UserPackageWithDependencies[]} */ (packages));
        }
        isLoading = false;
        console.log(`Loaded ${mode} packages from cache.`);
        return;
      }
    }
    
    console.log(`Fetching ${mode} packages from backend...`);
    packages = [];

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
        updateAvailableCategoriesAndSelection(userPackages);
      } else {
        result = await invoke('list_installed_packages');
        packages = /** @type {DisplayablePackage[]} */ (result); 
        packageCache.set(mode, result);
        availableCategoriesForFilter = [{ key: 'ALL', value: PackageCategory.ALL }]; // Reset for 'all' view
      }
    } catch (error) {
      console.error(`Error loading ${mode} packages:`, error);
      errorMessage = String(error);
    }
    isLoading = false;
  }

  /** @param {UserPackageWithDependencies[]} userPackages */
  function updateAvailableCategoriesAndSelection(userPackages) {
    const uniqueCategories = new Set();
    userPackages.forEach(pkg => {
      // Only add valid, known categories to the filter options, excluding UNKNOWN if desired
      if (pkg.category && Object.values(PackageCategory).includes(pkg.category) && pkg.category !== PackageCategory.UNKNOWN) {
        uniqueCategories.add(pkg.category);
      }
    });

    const sortedUniqueCategories = Array.from(uniqueCategories).sort((a, b) => {
        return formatCategoryName(/** @type {string} */ (a)).localeCompare(formatCategoryName(/** @type {string} */ (b)));
    });

    availableCategoriesForFilter = [
      { key: 'ALL', value: PackageCategory.ALL }, 
      ...sortedUniqueCategories.map(catValue => ({
        key: Object.keys(PackageCategory).find(k => PackageCategory[k] === catValue) || String(catValue), 
        value: String(catValue)
      }))
    ];
    
    // Check if the current selectedCategoryFilter is still valid
    const isValidSelection = availableCategoriesForFilter.some(cat => cat.value === selectedCategoryFilter);
    if (!isValidSelection) {
      selectedCategoryFilter = PackageCategory.ALL; // Reset to ALL if current selection is no longer available
    }
  }

  /** @param {'all' | 'user'} mode */
  function setViewMode(mode) {
    if (activeOperationCount > 0) {
        errorMessage = "Please wait for ongoing package operations to complete before changing views.";
        return;
    }
    packageViewMode = mode;
    searchTerm = ''; 
    selectedCategoryFilter = PackageCategory.ALL; // Reset category filter on view change
    if (mode === 'user') {
      // Ensure categories are updated based on the current state of 'packages' 
      // which might be from cache or will be fetched.
      // If packages is empty or not user packages, updateAvailableCategoriesAndSelection will handle it.
      updateAvailableCategoriesAndSelection(/** @type {UserPackageWithDependencies[]} */ (packageCache.get('user') || []));
    }
    fetchPackages(mode); 
  }

  function refreshCurrentView() {
    if (activeOperationCount > 0) {
        errorMessage = "Please wait for ongoing package operations to complete before refreshing.";
        return;
    }
    console.log('Forcing refresh for current view:', packageViewMode);
    searchTerm = ''; 
    // selectedCategoryFilter remains as is, user might want to refresh current filtered view
    fetchPackages(packageViewMode, true); // This will call updateAvailableCategoriesAndSelection internally
  }

  /** @param {string} packageName */
  function toggleDependencies(packageName) {
    packages = packages.map(pkg => {
      if ('dependencies' in pkg && pkg.name === packageName) {
        const userPkg = /** @type {UserPackageWithDependencies} */ (pkg);
        return { ...userPkg, showDependencies: !userPkg.showDependencies };
      }
      return pkg;
    });
  }

  $: filteredPackages = (() => {
    let tempFiltered = packages;

    // 1. Apply category filter (only for 'user' view)
    if (packageViewMode === 'user' && selectedCategoryFilter !== PackageCategory.ALL) {
      tempFiltered = tempFiltered.filter(pkg => {
        const userPkg = /** @type {UserPackageWithDependencies} */ (pkg);
        return userPkg.category === selectedCategoryFilter;
      });
    }

    // 2. Apply text search term
    if (!searchTerm.trim()) {
      return tempFiltered; 
    }
    const lowerSearchTerm = searchTerm.toLowerCase();
    return tempFiltered.filter(pkg => {
      const pkgNameLower = pkg.name.toLowerCase();
      if (pkgNameLower.includes(lowerSearchTerm)) {
        return true;
      }
      if (packageViewMode === 'user' && 'dependencies' in pkg) {
        const userPkg = /** @type {UserPackageWithDependencies} */ (pkg);
        if (userPkg.showDependencies && userPkg.dependencies) {
          return userPkg.dependencies.some(dep => 
            dep.name.toLowerCase().includes(lowerSearchTerm)
          );
        }
      }
      return false;
    });
  })();

  // Helper function to format category names for display
  /** @param {string} categoryValue */
  function formatCategoryName(categoryValue) {
    if (!categoryValue) return '';
    // Add spaces before capital letters, but not for the first letter if it's capital
    // and handle cases like "ALL" or single words.
    if (categoryValue === PackageCategory.ALL) return PackageCategory.ALL;
    return categoryValue.replace(/([A-Z])/g, ' $1').replace(/^\s+|\s+$/g, '').trim();
  }

  // Function to initiate a package operation status
  /** 
   * @param {string} packageName 
   * @param {boolean} isLoading
   * @param {string} [message='']
   * @param {boolean} [isError=false]
   * @param {string | null | undefined} [details]
  */
  function setPackageOpStatus(packageName, isLoading, message = '', isError = false, details = undefined) {
    packageOpStatus = {
      ...packageOpStatus,
      [packageName]: { isLoading, message, isError, details }
    };
    if (isLoading) activeOperationCount++; else if (activeOperationCount > 0) activeOperationCount--;
  }

  // Function to clear a package operation status
  /** @param {string} packageName */
  function clearPackageOpStatus(packageName) {
    const { [packageName]: _, ...rest } = packageOpStatus;
    packageOpStatus = rest;
    // Ensure activeOperationCount is decremented if it was loading
    // This might be redundant if setPackageOpStatus(false) was called, but good for safety
  }

  /**
   * @param {string} packageName
   * @param {'update'} action // Only 'update' now
   */
  async function handlePackageAction(packageName, action) {
    if (action !== 'update') {
      console.warn('handlePackageAction called with non-update action:', action);
      return; // Should not happen if UI is correct
    }
    const command = 'manage_package_update';
    const actionVerbGerund = 'updating';
    const actionVerbPast = 'updated';

    // Optional: Could add a simpler window.confirm for updates too if desired.
    // if (!window.confirm(`Are you sure you want to ${action} "${packageName}"?`)) return;

    setPackageOpStatus(packageName, true, `Attempting to ${action} ${packageName}...`);

    try {
      const result = /** @type {PackageOperationResultType} */ (await invoke(command, { packageName }));
      setPackageOpStatus(packageName, false, `${result.success ? 'Successfully' : 'Problem'} ${actionVerbPast} ${packageName}. ${result.message}`, !result.success, result.details);
      console.log(`Package ${action} ${result.success ? 'success' : 'failed'}:`, result.message, result.details);
      if (result.success) {
        await fetchPackages(packageViewMode, true); 
      }
    } catch (error) {
      const errorMsg = String(error);
      setPackageOpStatus(packageName, false, `Error ${actionVerbGerund} ${packageName}: ${errorMsg}`, true, errorMsg);
      console.error(`Package ${action} error:`, error);
    }
    setTimeout(() => {
        if (packageOpStatus[packageName] && !packageOpStatus[packageName].isLoading) {
            clearPackageOpStatus(packageName);
        }
    }, 8000);
  }

  /** @param {string} pkgName */
  function openUninstallModal(pkgName) {
    if (activeOperationCount > 0 && !packageOpStatus[pkgName]?.isLoading) {
      errorMessage = "Please wait for other ongoing package operations to complete.";
      setTimeout(() => errorMessage = '', 3000);
      return;
    }
    packageForUninstall = pkgName;
    isUninstallModalOpen = true;
  }

  function handleUninstallCompleted() {
    isUninstallModalOpen = false;
    packageForUninstall = '';
    fetchPackages(packageViewMode, true); 
  }

  // Ensure onMount doesn't run fetch if ops are active (though unlikely on initial mount)
  onMount(() => {
    if (activeOperationCount === 0) {
        fetchPackages(packageViewMode); // This will call updateAvailableCategoriesAndSelection for user view
    }
  });

  // Cleanup active operations if component is destroyed (e.g. navigation)
  onDestroy(() => {
    activeOperationCount = 0; 
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

  .filter-controls-container {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 20px;
    margin-bottom: 25px;
  }

  .search-container {
    flex-grow: 1;
    margin-bottom: 0;
    display: flex; /* Added to center the search input */
    justify-content: flex-start; /* Aligns search input to the left within its flexible space */
    /* If you prefer the search input itself to be centered when it's narrower than its container: */
    /* justify-content: center; */ 
  }

  .search-input {
    width: 100%; 
    max-width: 500px; /* It will not exceed this width */
  }
  
  .category-filter-container {
  display: flex;
    align-items: center;
    gap: 8px;
    /* flex-shrink: 0; /* Prevent dropdown from shrinking too much */
  }

  .category-filter-container label {
    color: var(--nebula-text-secondary);
    font-size: 0.9em;
    white-space: nowrap; /* Prevent label from wrapping */
  }

  .category-select-wrapper {
    position: relative;
    display: inline-block;
  }

  .category-select {
    padding: 10px 35px 10px 15px; /* Adjusted padding for custom arrow */
    background-color: var(--nebula-surface);
    color: var(--nebula-text-primary);
    border: 1px solid var(--nebula-border);
    border-radius: 20px;
    font-size: 0.9em;
    min-width: 220px; /* Give it some base width */
    transition: border-color 0.3s ease, box-shadow 0.3s ease;
    appearance: none; /* Remove default arrow */
    -webkit-appearance: none;
    -moz-appearance: none;
    cursor: pointer;
  }
  .category-select-wrapper::after { /* Custom arrow */
    content: '';
    position: absolute;
    top: 50%;
    right: 15px;
    transform: translateY(-50%) rotate(45deg);
    width: 7px;
    height: 7px;
    border-bottom: 2px solid var(--nebula-accent);
    border-right: 2px solid var(--nebula-accent);
    pointer-events: none; /* So it doesn't interfere with select click */
    transition: border-color 0.3s ease;
  }
  .category-select-wrapper:hover::after {
      border-color: var(--nebula-accent-hover);
  }

  .category-select:focus {
    outline: none;
    border-color: var(--nebula-accent);
    box-shadow: 0 0 8px var(--nebula-accent);
  }

  .package-category-badge {
    font-size: 0.75em;
    padding: 3px 8px;
    border-radius: 10px;
    background-color: var(--nebula-surface-light); /* Default */
    color: var(--nebula-text-secondary); /* Default */
    margin-left: 10px;
    border: 1px solid var(--nebula-border); /* Default */
    /* text-transform: capitalize; Replaced by helper function for better formatting */
    display: inline-block; /* Ensures proper spacing and layout */
    vertical-align: middle;
  }
  /* Style for specific categories for more visual distinction */
  .category-badge-Manual { background-color: #3a5fcd; color: white; } /* Medium Blue */
  .category-badge-DesktopEnvironment { background-color: #cd5c5c; color: white; } /* Indian Red */
  .category-badge-System { background-color: #6a6a6a; color: white; } /* Dim Gray */
  .category-badge-Library { background-color: #487f48; color: white; } /* Dark Green */
  .category-badge-Development { background-color: #b8860b; color: white; } /* DarkGoldenRod */
  .category-badge-Multimedia { background-color: #9370db; color: white; } /* MediumPurple */
  .category-badge-Office { background-color: #ff8c00; color: white; } /* DarkOrange */
  .category-badge-Games { background-color: #ff1493; color: white; } /* DeepPink */
  .category-badge-Utility { background-color: #00ced1; color: black; } /* DarkTurquoise */
  .category-badge-Network { background-color: #1e90ff; color: white; } /* DodgerBlue */
  .category-badge-Security { background-color: #dc143c; color: white; } /* Crimson */
  .category-badge-OtherApplication { background-color: #da70d6; color: white; } /* Orchid */
  .category-badge-Unknown { background-color: #778899; color: white; } /* LightSlateGray */

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
  }

  .package-header {
  cursor: pointer;
    padding: 8px 0;
    border-radius: 5px;
    display: flex;
    justify-content: space-between;
    align-items: flex-start; /* Align items to top if actions wrap */
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
    list-style-type: none; 
    border-top: 1px dashed var(--nebula-border);
    padding-top: 12px;
  }

  .dependencies-list li {
    padding: 4px 0;
    color: var(--nebula-text-secondary);
    position: relative;
  }
  .dependencies-list li::before {
    content: '◆'; 
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

  .package-actions button {
    padding: 5px 10px;
    font-size: 0.8em;
    margin-left: 8px;
    border-radius: 15px;
    /* Use existing button vars or define new ones for smaller actions */
  }

  .package-actions .update-btn {
    background-color: var(--nebula-green-glow); /* Or a blue similar to accent */
    color: #002b00; /* Dark green text */
  }
  .package-actions .update-btn:hover {
    background-color: #33ffbb;
    box-shadow: 0 0 10px #33ffbb;
  }

  .package-actions .remove-btn {
    background-color: var(--nebula-red-glow);
    color: #330000; /* Dark red text */
  }
  .package-actions .remove-btn:hover {
    background-color: #ff7777;
    box-shadow: 0 0 10px #ff7777;
  }
  .package-actions button:disabled {
    background-color: var(--nebula-border);
    color: var(--nebula-text-secondary);
    opacity: 0.5;
    cursor: not-allowed;
  }

  .package-op-status {
    font-size: 0.8em;
    margin-top: 5px;
    padding: 5px;
    border-radius: 4px;
    word-break: break-all; /* For long error messages */
  }
  .package-op-status.loading {
    color: var(--nebula-accent);
    background-color: rgba(255,0,170,0.1);
  }
  .package-op-status.success {
    color: var(--nebula-green-glow);
    background-color: rgba(0,255,170,0.1);
  }
  .package-op-status.error {
    color: var(--nebula-red-glow);
    background-color: rgba(255,85,85,0.1);
  }

  .package-info {
    flex-grow: 1; /* Allow package name and badge to take space */
  }

  .package-controls {
    display: flex; /* For header content */
    justify-content: space-between;
    align-items: center; /* Vertically center name/badge with dep count */
    width: 100%;
  }
</style>

<UninstallModal 
  bind:isOpen={isUninstallModalOpen} 
  packageName={packageForUninstall}
  on:close={() => { isUninstallModalOpen = false; packageForUninstall = ''; }}
  on:uninstallCompleted={handleUninstallCompleted}
/>

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

  <div class="filter-controls-container">
    <div class="search-container">
      <input 
        type="text" 
        bind:value={searchTerm} 
        placeholder="Search packages or dependencies..." 
        class="search-input"
        aria-label="Search packages and dependencies"
      />
    </div>

    {#if packageViewMode === 'user'}
      <div class="category-filter-container">
        <label for="category-filter">Category:</label> <!-- Shortened Label -->
        <div class="category-select-wrapper"> <!-- Wrapper for custom arrow -->
          <select id="category-filter" bind:value={selectedCategoryFilter} class="category-select" disabled={isLoading || availableCategoriesForFilter.length <= 1}>
            {#each availableCategoriesForFilter as category (category.key)}
              <option value={category.value}>{formatCategoryName(category.value)}</option>
            {/each}
          </select>
        </div>
      </div>
    {/if}
  </div>

  {#if errorMessage && activeOperationCount === 0} <!-- Only show global error if no package ops causing it -->
    <p class="error-message" role="alert">Error: {errorMessage}</p>
  {/if}

  {#if isLoading}
    <p class="loading-message">Summoning data from the DNF void for {packageViewMode === 'user' ? 'user installed' : 'all'} packages...</p>
  {:else if packages.length > 0 && filteredPackages.length > 0}
    <h2>
      {packageViewMode === 'user' ? 
        `${formatCategoryName(selectedCategoryFilter) === PackageCategory.ALL ? 'User Installed' : formatCategoryName(selectedCategoryFilter)} Packages` : 
        'All Installed Packages'}
      ({filteredPackages.length}{searchTerm.trim() ? ` matching '${searchTerm}'` : ''} / {packages.length} total):
    </h2>
    <ul class="packages-list">
      {#each filteredPackages as pkg (pkg.name)} 
        {#if packageViewMode === 'user'}
          {@const userPkg = /** @type {UserPackageWithDependencies} */ (pkg)}
          <li class="package-item">
            <div class="package-header" 
              on:click={(e) => {
                  // Prevent toggling dependencies if a button inside package-actions was clicked
                  const eventTarget = /** @type {HTMLElement} */ (e.target);
                  if (eventTarget && eventTarget.closest('.package-actions')) return;
                  toggleDependencies(userPkg.name);
              }}
              role="button" 
              tabindex="0" 
              on:keydown={(e) => {
                  const eventTarget = /** @type {HTMLElement} */ (e.target);
                  if (eventTarget && eventTarget.closest('.package-actions')) return;
                  if (e.key === 'Enter') toggleDependencies(userPkg.name);
              }}>
              
              <div class="package-controls">
                <div class="package-info">
                  <strong>{userPkg.name}</strong>
                  <span class={`package-category-badge category-badge-${userPkg.category}`}>{formatCategoryName(userPkg.category)}</span>
                </div>
                <span class="dep-count">
                  {userPkg.showDependencies ? 'Hide' : 'Show'} {userPkg.dependencies.length} Dependencies
                </span>
              </div>

              {#if !packageOpStatus[userPkg.name]?.isLoading && !isUninstallModalOpen}
                <div class="package-actions">
                  <button 
                    class="update-btn"
                    on:click|stopPropagation={() => handlePackageAction(userPkg.name, 'update')} 
                    title={`Update ${userPkg.name}`}
                    disabled={activeOperationCount > 0 || (packageOpStatus[userPkg.name]?.isLoading)}>
                    Update
                  </button>
                  <button 
                    class="remove-btn"
                    on:click|stopPropagation={() => openUninstallModal(userPkg.name)} 
                    title={`Remove ${userPkg.name}`}
                    disabled={activeOperationCount > 0 || (packageOpStatus[userPkg.name]?.isLoading)}>
                    Remove...
                  </button>
                </div>
              {/if}
            </div>

            {#if packageOpStatus[userPkg.name]?.message && !isUninstallModalOpen}
              <div 
                class={`package-op-status ${packageOpStatus[userPkg.name]?.isLoading ? 'loading' : (packageOpStatus[userPkg.name]?.isError ? 'error' : 'success')}`}
                role="status"
                aria-live="polite">
                {packageOpStatus[userPkg.name]?.isLoading ? 'Processing...' : ''} {packageOpStatus[userPkg.name]?.message}
                {#if packageOpStatus[userPkg.name]?.details && packageOpStatus[userPkg.name]?.isError}
                  <pre class="error-details">{packageOpStatus[userPkg.name]?.details}</pre>
                {/if}
              </div>
            {/if}

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
  {:else if packages.length > 0 && (searchTerm.trim() || (packageViewMode === 'user' && selectedCategoryFilter !== PackageCategory.ALL))}
    <p class="no-packages-message">
      No packages match your current filter criteria 
      {#if searchTerm.trim()}(search: "{searchTerm}"){/if}
      {#if packageViewMode === 'user' && selectedCategoryFilter !== PackageCategory.ALL}(category: "{formatCategoryName(selectedCategoryFilter)}"){/if}.
      Clear filters to see all {packages.length} packages.
    </p>
  {:else if !errorMessage} 
    <p class="no-packages-message">The DNF void seems empty for {packageViewMode === 'user' ? 'user installed' : 'all'} packages.</p>
  {/if}
</div>

<svelte:head>
  <title>Nebula DNF Explorer</title>
</svelte:head>
