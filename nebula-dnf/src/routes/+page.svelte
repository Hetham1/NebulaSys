<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import UninstallModal from './UninstallModal.svelte'; // Import the modal
  import '../theme.css'; // Import the new theme CSS

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

  /** @param {string} packageName */
  function showDetails(packageName) {
    const status = packageOpStatus[packageName];
    if (status && status.details) {
      // For simplicity, using alert. In a real app, use a modal or a dedicated area.
      alert(`Details for ${packageName}:\n\n${status.details}`);
    } else {
      alert(`No details available for ${packageName}.`);
    }
  }
</script>

<svelte:head>
  <title>NebulaSys - Package Manager</title>
</svelte:head>

<div class="container">
  <header class="app-header">
    <h1>NebulaSys Package Manager</h1>
  </header>

  {#if errorMessage}
    <div class="error-message floating-message">
      <p>{errorMessage}</p>
      <button on:click={() => errorMessage = ''}>Dismiss</button>
    </div>
  {/if}

  <div class="controls">
    <div class="view-switcher">
      <button 
        class:active={packageViewMode === 'user'} 
        on:click={() => setViewMode('user')}
        disabled={activeOperationCount > 0}>
        User Installed
    </button>
      <button 
        class:active={packageViewMode === 'all'} 
        on:click={() => setViewMode('all')}
        disabled={activeOperationCount > 0}>
      All Packages
    </button>
  </div>
      <input 
        type="text" 
      placeholder="Search packages..." 
        bind:value={searchTerm} 
        class="search-input"
      />
    {#if packageViewMode === 'user'}
      <select bind:value={selectedCategoryFilter} class="category-filter">
        {#each availableCategoriesForFilter as cat (cat.value)}
          <option value={cat.value}>{formatCategoryName(cat.value)}</option>
            {/each}
          </select>
    {/if}
    <button class="refresh-button" on:click={refreshCurrentView} disabled={activeOperationCount > 0 || isLoading}>
      {#if isLoading && activeOperationCount === 0}
        Loading...
      {:else}
        Refresh
      {/if}
    </button>
  </div>

  {#if isLoading && packages.length === 0}
    <div class="loading-indicator">
      <div class="spinner"></div>
      <p>Loading packages...</p>
    </div>
  {:else if filteredPackages.length === 0 && !isLoading}
    <div class="empty-state">
      <p>No packages found matching your criteria.</p>
    </div>
  {:else}
    <ul class="package-list">
      {#each filteredPackages as pkg (pkg.name)} 
        {@const status = packageOpStatus[pkg.name]}
        <li class="package-item" class:has-op-error={status?.isError} class:has-op-success={status && !status.isLoading && !status.isError}>
          <div class="package-info">
            <span class="package-name">{pkg.name}</span>
            {#if packageViewMode === 'user' && 'category' in pkg}
          {@const userPkg = /** @type {UserPackageWithDependencies} */ (pkg)}
              <span class="package-category" title={userPkg.category}>{formatCategoryName(userPkg.category)}</span>
            {/if}
              </div>

                <div class="package-actions">
            {#if packageViewMode === 'user'}
             {#if 'dependencies' in pkg}
                {@const userPkg = /** @type {UserPackageWithDependencies} */ (pkg)}
                {#if userPkg.dependencies && userPkg.dependencies.length > 0}
                  <button 
                    class="action-button"
                    on:click={() => toggleDependencies(pkg.name)}
                    title={userPkg.showDependencies ? "Hide Dependencies" : "Show Dependencies"}>
                    {userPkg.showDependencies ? 'Hide Deps' : 'Show Deps'} ({userPkg.dependencies.length})
                  </button>
                {/if}
              {/if}
              <button 
                class="action-button update-button" 
                on:click={() => handlePackageAction(pkg.name, 'update')}
                disabled={status?.isLoading || activeOperationCount > 0}
                title="Update this package">
                {#if status?.isLoading && status.message.toLowerCase().includes('updat')}Updating...{:else}Update{/if}
                  </button>
                  <button 
                class="action-button uninstall-button" 
                on:click={() => openUninstallModal(pkg.name)}
                disabled={status?.isLoading || activeOperationCount > 0}
                title="Uninstall this package">
                {#if status?.isLoading && status.message.toLowerCase().includes('uninstall')}Uninstalling...{:else}Uninstall{/if}
                  </button>
              {/if}
            </div>

          {#if status && (status.message || status.details)}
            <div class="package-status" class:error={status.isError} class:success={!status.isError && !status.isLoading}>
                <span>{status.message}</span>
                {#if status.details}
                    <button class="details-button" on:click={() => showDetails(pkg.name)}>Details</button>
                {/if}
                 {#if !status.isLoading}
                    <button class="dismiss-status-button" on:click={() => {delete packageOpStatus[pkg.name]; packageOpStatus = packageOpStatus;}} title="Dismiss status">&times;</button>
                {/if}
              </div>
            {/if}

          {#if packageViewMode === 'user' && 'dependencies' in pkg}
            {@const userPkg = /** @type {UserPackageWithDependencies} */ (pkg)}
            {#if userPkg.showDependencies && userPkg.dependencies && userPkg.dependencies.length > 0}
              <ul class="dependencies-list">
                {#each userPkg.dependencies as dep (dep.name)}
                  <li class="dependency-item">{dep.name}</li>
                {/each}
              </ul>
            {/if}
            {/if}
          </li>
      {/each}
    </ul>
  {/if}
</div>

<UninstallModal 
  bind:isOpen={isUninstallModalOpen} 
  packageName={packageForUninstall}
  on:uninstallCompleted={handleUninstallCompleted}
  on:close={() => isUninstallModalOpen = false}
/>
