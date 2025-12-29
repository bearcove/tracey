// Editor configuration
const EDITORS = {
  zed: { name: "Zed", urlTemplate: (path, line) => `zed://file/${path}:${line}` },
  vscode: { name: "VS Code", urlTemplate: (path, line) => `vscode://file/${path}:${line}` },
  idea: { name: "IntelliJ", urlTemplate: (path, line) => `idea://open?file=${path}&line=${line}` },
  sublime: { name: "Sublime", urlTemplate: (path, line) => `subl://open?url=file://${path}&line=${line}` },
};

function getEditor() {
  return localStorage.getItem('tracey-editor') || 'zed';
}

function updateAllLinks() {
  const editor = getEditor();
  const config = EDITORS[editor];
  
  document.querySelectorAll('.file-link').forEach(link => {
    const path = link.dataset.path;
    const line = link.dataset.line || '1';
    const fullPath = PROJECT_ROOT + '/' + path;
    link.href = config.urlTemplate(fullPath, line);
  });
}

// Tab switching
function showTab(tabId) {
  // Update tab buttons
  document.querySelectorAll('.tab').forEach(tab => {
    tab.classList.toggle('active', tab.textContent.toLowerCase().includes(tabId));
  });
  
  // Update panels
  document.querySelectorAll('.panel').forEach(panel => {
    panel.classList.toggle('active', panel.id === tabId + '-panel');
  });
}

// Live reload
let currentVersion = null;
let pollInterval = null;

async function checkForUpdates() {
  try {
    const response = await fetch('/__version');
    if (response.ok) {
      const version = await response.text();
      if (currentVersion === null) {
        currentVersion = version;
      } else if (version !== currentVersion) {
        // Version changed, reload the page
        showReloadOverlay();
        setTimeout(() => {
          window.location.reload();
        }, 300);
      }
    }
  } catch (e) {
    // Server might be restarting, ignore
  }
}

function showReloadOverlay() {
  const overlay = document.getElementById('reload-overlay');
  if (overlay) {
    overlay.classList.add('visible');
  }
}

function startPolling() {
  // Poll every 500ms for updates
  pollInterval = setInterval(checkForUpdates, 500);
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
  updateAllLinks();
  startPolling();
  
  // Add reload overlay to body
  const overlay = document.createElement('div');
  overlay.id = 'reload-overlay';
  overlay.className = 'reload-overlay';
  overlay.innerHTML = '<div class="reload-message"><div class="reload-spinner"></div>Reloading...</div>';
  document.body.appendChild(overlay);
});

// Keyboard shortcuts
document.addEventListener('keydown', (e) => {
  // Switch tabs with 1/2 keys
  if (e.key === '1' && !e.ctrlKey && !e.metaKey) {
    showTab('forward');
  } else if (e.key === '2' && !e.ctrlKey && !e.metaKey) {
    showTab('reverse');
  }
});
