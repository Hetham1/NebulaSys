<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import {
    AlertTriangle,
    ChevronDown,
    ChevronRight,
    Download,
    Layers,
    PackageOpen,
    RefreshCw,
    Search,
    ShieldCheck,
    Trash2
  } from '@lucide/svelte';
  import OperationModal from './OperationModal.svelte';
  import '../theme.css';

  /**
   * @typedef {Object} ManagerStatus
   * @property {string} id
   * @property {string} label
   * @property {boolean} installed
   * @property {string} executable
   * @property {string | null | undefined} version
   * @property {string} notes
   * @property {boolean} supports_user_installed
   * @property {boolean} supports_dependencies
   * @property {boolean} supports_update
   * @property {boolean} supports_uninstall
   * @property {boolean} supports_force_uninstall
   * @property {boolean} supports_cleanup_orphans
   * @property {boolean} requires_privilege
   */

  /**
   * @typedef {Object} DependencyInfo
   * @property {string} name
   * @property {string} kind
   */

  /**
   * @typedef {Object} PackageInfo
   * @property {string} manager
   * @property {string} name
   * @property {string} display_name
   * @property {string | null | undefined} version
   * @property {string} category
   * @property {string | null | undefined} summary
   * @property {string} source
   * @property {DependencyInfo[]} dependencies
   * @property {boolean} dependencies_loaded
   */

  const MANAGER_ORDER = ['dnf', 'apt', 'snap', 'flatpak'];
  const ALL_CATEGORIES = 'All categories';

  /** @type {ManagerStatus[]} */
  let managerStatuses = [];
  /** @type {string} */
  let selectedManager = 'dnf';
  /** @type {'user' | 'all'} */
  let packageViewMode = 'user';
  /** @type {PackageInfo[]} */
  let packages = [];
  let isLoading = true;
  let errorMessage = '';
  let searchTerm = '';
  let selectedCategory = ALL_CATEGORIES;
  /** @type {Map<string, PackageInfo[]>} */
  let packageCache = new Map();
  /** @type {Set<string>} */
  let expandedRows = new Set();
  /** @type {Record<string, boolean>} */
  let dependencyLoading = {};

  let operationModalOpen = false;
  /** @type {PackageInfo | null} */
  let operationPackage = null;
  /** @type {'update' | 'uninstall'} */
  let operationType = 'update';

  $: selectedManagerStatus = managerStatuses.find((manager) => manager.id === selectedManager);
  $: selectedManagerAvailable = Boolean(selectedManagerStatus?.installed);
  $: selectedManagerLabel = selectedManagerStatus?.label || selectedManager.toUpperCase();
  $: categories = [
    ALL_CATEGORIES,
    ...Array.from(new Set(packages.map((pkg) => pkg.category).filter(Boolean))).sort()
  ];
  $: filteredPackages = filterPackages(packages, searchTerm, selectedCategory);
  $: availableManagers = managerStatuses.filter((manager) => manager.installed).length;
  $: dependencyLoadingCount = Object.values(dependencyLoading).filter(Boolean).length;

  /** @param {PackageInfo} pkg */
  function packageKey(pkg) {
    return `${pkg.manager}:${pkg.name}`;
  }

  /** @param {string} manager @param {'user' | 'all'} view */
  function cacheKey(manager, view) {
    return `${manager}:${view}`;
  }

  function currentManagerStatus() {
    return managerStatuses.find((manager) => manager.id === selectedManager);
  }

  /** @param {PackageInfo[]} sourcePackages @param {string} query @param {string} category */
  function filterPackages(sourcePackages, query, category) {
    const normalizedQuery = query.trim().toLowerCase();
    return sourcePackages.filter((pkg) => {
      if (category !== ALL_CATEGORIES && pkg.category !== category) {
        return false;
      }
      if (!normalizedQuery) {
        return true;
      }
      return [
        pkg.name,
        pkg.display_name,
        pkg.version,
        pkg.category,
        pkg.summary,
        pkg.source,
        ...(pkg.dependencies || []).map((dep) => dep.name)
      ]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(normalizedQuery));
    });
  }

  async function initializeApp() {
    isLoading = true;
    errorMessage = '';
    try {
      managerStatuses = /** @type {ManagerStatus[]} */ (await invoke('get_manager_statuses'));
      const firstAvailable = MANAGER_ORDER.find((id) =>
        managerStatuses.some((manager) => manager.id === id && manager.installed)
      );
      selectedManager = firstAvailable || 'dnf';
      const status = managerStatuses.find((manager) => manager.id === selectedManager);
      packageViewMode = status?.supports_user_installed ? 'user' : 'all';
      await fetchPackages(false);
    } catch (error) {
      errorMessage = String(error);
      packages = [];
    } finally {
      isLoading = false;
    }
  }

  async function fetchPackages(forceRefresh = false) {
    const status = currentManagerStatus();
    if (!status?.installed) {
      packages = [];
      return;
    }

    const key = cacheKey(selectedManager, packageViewMode);
    if (!forceRefresh && packageCache.has(key)) {
      packages = packageCache.get(key) || [];
      return;
    }

    isLoading = true;
    errorMessage = '';
    try {
      const result = /** @type {PackageInfo[]} */ (await invoke('list_packages', {
        manager: selectedManager,
        view: packageViewMode,
        forceRefresh
      }));
      packages = result;
      packageCache.set(key, result);
      expandedRows = new Set();
      selectedCategory = ALL_CATEGORIES;
    } catch (error) {
      errorMessage = String(error);
      packages = [];
    } finally {
      isLoading = false;
    }
  }

  /** @param {string} managerId */
  async function selectManager(managerId) {
    if (selectedManager === managerId) return;
    selectedManager = managerId;
    const status = managerStatuses.find((manager) => manager.id === managerId);
    packageViewMode = status?.supports_user_installed ? 'user' : 'all';
    searchTerm = '';
    selectedCategory = ALL_CATEGORIES;
    await fetchPackages(false);
  }

  /** @param {'user' | 'all'} view */
  async function setViewMode(view) {
    if (packageViewMode === view) return;
    packageViewMode = view;
    selectedCategory = ALL_CATEGORIES;
    searchTerm = '';
    await fetchPackages(false);
  }

  async function refreshCurrentView() {
    packageCache.delete(cacheKey(selectedManager, packageViewMode));
    await fetchPackages(true);
  }

  /** @param {PackageInfo} pkg */
  async function toggleRequirements(pkg) {
    const key = packageKey(pkg);
    if (expandedRows.has(key)) {
      expandedRows.delete(key);
      expandedRows = new Set(expandedRows);
      return;
    }

    expandedRows.add(key);
    expandedRows = new Set(expandedRows);

    if (pkg.dependencies_loaded) return;

    dependencyLoading = { ...dependencyLoading, [key]: true };
    try {
      const dependencies = /** @type {DependencyInfo[]} */ (await invoke('get_package_dependencies', {
        manager: pkg.manager,
        packageName: pkg.name
      }));
      packages = packages.map((candidate) =>
        packageKey(candidate) === key
          ? { ...candidate, dependencies, dependencies_loaded: true }
          : candidate
      );
      packageCache.set(cacheKey(selectedManager, packageViewMode), packages);
    } catch (error) {
      errorMessage = String(error);
      packages = packages.map((candidate) =>
        packageKey(candidate) === key
          ? { ...candidate, dependencies: [], dependencies_loaded: true }
          : candidate
      );
    } finally {
      dependencyLoading = { ...dependencyLoading, [key]: false };
    }
  }

  /** @param {PackageInfo} pkg @param {'update' | 'uninstall'} operation */
  function openOperation(pkg, operation) {
    operationPackage = pkg;
    operationType = operation;
    operationModalOpen = true;
  }

  async function handleOperationCompleted() {
    operationModalOpen = false;
    operationPackage = null;
    await refreshCurrentView();
  }

  onMount(initializeApp);
</script>

<svelte:head>
  <title>NebulaSys</title>
</svelte:head>

<div class="app-shell">
  <aside class="sidebar" aria-label="Package managers">
    <div class="brand-block">
      <div class="brand-mark"><PackageOpen size={22} /></div>
      <div>
        <strong>NebulaSys</strong>
        <span>Package control</span>
      </div>
    </div>

    <nav class="manager-list">
      {#each managerStatuses as manager (manager.id)}
        <button
          class:active={selectedManager === manager.id}
          class:unavailable={!manager.installed}
          on:click={() => selectManager(manager.id)}
        >
          <span>{manager.label}</span>
          <small>{manager.installed ? 'Available' : 'Missing'}</small>
        </button>
      {/each}
    </nav>

    <div class="sidebar-status">
      <ShieldCheck size={18} />
      <span>{availableManagers} of {managerStatuses.length || 4} managers detected</span>
    </div>
  </aside>

  <main class="workspace">
    <header class="workspace-header">
      <div>
        <p>{selectedManagerLabel}</p>
        <h1>Installed packages</h1>
        {#if selectedManagerStatus}
          <span>{selectedManagerStatus.notes}</span>
        {/if}
      </div>
      <button class="toolbar-button" on:click={refreshCurrentView} disabled={isLoading || !selectedManagerAvailable}>
        <RefreshCw size={17} />
        Refresh
      </button>
    </header>

    <section class="status-grid" aria-label="Package manager status">
      <div>
        <span>Packages</span>
        <strong>{packages.length}</strong>
      </div>
      <div>
        <span>Visible</span>
        <strong>{filteredPackages.length}</strong>
      </div>
      <div>
        <span>Version</span>
        <strong>{selectedManagerStatus?.version || 'Unknown'}</strong>
      </div>
      <div>
        <span>Privilege</span>
        <strong>{selectedManagerStatus?.requires_privilege ? 'Polkit' : 'User session'}</strong>
      </div>
    </section>

    {#if errorMessage}
      <div class="notice error-notice" role="alert">
        <AlertTriangle size={18} />
        <span>{errorMessage}</span>
        <button on:click={() => (errorMessage = '')}>Dismiss</button>
      </div>
    {/if}

    <section class="toolbar" aria-label="Package filters">
      <div class="segmented-control">
        <button
          class:active={packageViewMode === 'user'}
          on:click={() => setViewMode('user')}
          disabled={!selectedManagerStatus?.supports_user_installed || isLoading}
        >
          User installed
        </button>
        <button
          class:active={packageViewMode === 'all'}
          on:click={() => setViewMode('all')}
          disabled={isLoading}
        >
          All packages
        </button>
      </div>

      <label class="search-field">
        <Search size={17} />
        <input type="text" bind:value={searchTerm} placeholder="Search packages" />
      </label>

      <select bind:value={selectedCategory} disabled={categories.length <= 1}>
        {#each categories as category}
          <option value={category}>{category}</option>
        {/each}
      </select>
    </section>

    {#if !selectedManagerAvailable && !isLoading}
      <section class="empty-panel">
        <PackageOpen size={30} />
        <h2>{selectedManagerLabel} is not available</h2>
        <p>Install the package manager on this system and refresh NebulaSys.</p>
      </section>
    {:else if isLoading && packages.length === 0}
      <section class="loading-panel">
        <div class="spinner"></div>
        <p>Loading {selectedManagerLabel} packages</p>
      </section>
    {:else if filteredPackages.length === 0}
      <section class="empty-panel">
        <Search size={30} />
        <h2>No packages found</h2>
        <p>Adjust the search or category filter.</p>
      </section>
    {:else}
      <section class="package-table" aria-label="Installed packages">
        <div class="table-header">
          <span>Package</span>
          <span>Version</span>
          <span>Category</span>
          <span>Actions</span>
        </div>

        {#each filteredPackages as pkg (packageKey(pkg))}
          {@const key = packageKey(pkg)}
          {@const isExpanded = expandedRows.has(key)}
          <article class="package-row">
            <div class="package-main">
              <button class="expand-button" on:click={() => toggleRequirements(pkg)} aria-label="Toggle requirements">
                {#if isExpanded}
                  <ChevronDown size={17} />
                {:else}
                  <ChevronRight size={17} />
                {/if}
              </button>
              <div>
                <strong>{pkg.display_name || pkg.name}</strong>
                <code>{pkg.name}</code>
                {#if pkg.summary}
                  <p>{pkg.summary}</p>
                {/if}
              </div>
            </div>

            <span class="version-cell">{pkg.version || 'Unknown'}</span>
            <span class="category-chip">{pkg.category || 'Uncategorized'}</span>

            <div class="row-actions">
              <button
                class="icon-action"
                on:click={() => toggleRequirements(pkg)}
                disabled={dependencyLoading[key]}
                title="Requirements"
              >
                <Layers size={16} />
                {dependencyLoading[key] ? 'Loading' : 'Requirements'}
              </button>
              <button
                class="icon-action"
                on:click={() => openOperation(pkg, 'update')}
                disabled={!selectedManagerStatus?.supports_update}
                title="Update package"
              >
                <Download size={16} />
                Update
              </button>
              <button
                class="icon-action danger"
                on:click={() => openOperation(pkg, 'uninstall')}
                disabled={!selectedManagerStatus?.supports_uninstall}
                title="Uninstall package"
              >
                <Trash2 size={16} />
                Uninstall
              </button>
            </div>

            {#if isExpanded}
              <div class="requirements-panel">
                {#if dependencyLoading[key]}
                  <p>Loading requirements...</p>
                {:else if pkg.dependencies && pkg.dependencies.length > 0}
                  <ul>
                    {#each pkg.dependencies as dep (`${dep.kind}:${dep.name}`)}
                      <li>
                        <span>{dep.name}</span>
                        <small>{dep.kind}</small>
                      </li>
                    {/each}
                  </ul>
                {:else}
                  <p>No requirement data available for this package.</p>
                {/if}
              </div>
            {/if}
          </article>
        {/each}
      </section>
    {/if}

    {#if dependencyLoadingCount > 0}
      <div class="floating-status">{dependencyLoadingCount} requirement query running</div>
    {/if}
  </main>
</div>

<OperationModal
  bind:isOpen={operationModalOpen}
  packageInfo={operationPackage}
  managerStatus={selectedManagerStatus}
  operation={operationType}
  on:completed={handleOperationCompleted}
  on:close={() => (operationModalOpen = false)}
/>
