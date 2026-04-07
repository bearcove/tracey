(function () {
  // r[impl export.sidebar.mobile]
  // Mobile sidebar toggle
  var toggle = document.getElementById("sidebar-toggle");
  var sidebar = document.querySelector(".sidebar");
  if (toggle && sidebar) {
    var backdrop = document.createElement("div");
    backdrop.className = "sidebar-backdrop";
    document.body.appendChild(backdrop);

    function openSidebar() {
      sidebar.classList.add("open");
      backdrop.classList.add("open");
    }
    function closeSidebar() {
      sidebar.classList.remove("open");
      backdrop.classList.remove("open");
    }

    toggle.addEventListener("click", function () {
      sidebar.classList.contains("open") ? closeSidebar() : openSidebar();
    });
    backdrop.addEventListener("click", closeSidebar);
  }

  // r[impl export.sidebar.collapsible]
  // Persist collapsed state in localStorage
  var STORAGE_KEY = "tracey-export-sidebar-state";
  var allCollapsible = ".sidebar-section, .sidebar-subsection";

  function saveSidebarState() {
    var state = {};
    document.querySelectorAll(allCollapsible).forEach(function (el) {
      var key = el.dataset.sidebarKey;
      if (key) state[key] = el.open;
    });
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch (e) {}
  }

  function restoreSidebarState() {
    try {
      var state = JSON.parse(localStorage.getItem(STORAGE_KEY));
      if (!state) return;
      document.querySelectorAll(allCollapsible).forEach(function (el) {
        var key = el.dataset.sidebarKey;
        if (key && key in state) el.open = state[key];
      });
    } catch (e) {}
  }

  // r[impl export.sidebar.current-page]
  // Scroll spy: highlight the current section in the sidebar and
  // auto-expand its parent <details> as you scroll.
  function setupScrollSpy() {
    // The content panel is the scroll container (app-style layout),
    // aligned with the dashboard's approach in spec.tsx.
    var contentPanel = document.querySelector(".content");
    if (!contentPanel || !sidebar) return;

    // Build a map from slug -> sidebar link
    var sidebarLinks = Array.from(sidebar.querySelectorAll("a[href*='#']"));
    if (sidebarLinks.length === 0) return;

    var linkBySlug = {};
    sidebarLinks.forEach(function (a) {
      var hash = a.getAttribute("href");
      var idx = hash.indexOf("#");
      if (idx >= 0) {
        var slug = hash.slice(idx + 1);
        if (slug) linkBySlug[slug] = a;
      }
    });

    var currentSlug = null;

    function updateScrollSpy() {
      // Query headings directly from the content (same as dashboard)
      var headingEls = contentPanel.querySelectorAll("h1[id], h2[id], h3[id], h4[id]");
      if (headingEls.length === 0) return;

      var scrollTop = contentPanel.scrollTop;
      var viewportTop = 100; // offset, same as dashboard

      var best = null;
      for (var i = 0; i < headingEls.length; i++) {
        var el = headingEls[i];
        // offsetTop relative to offsetParent; we need position relative
        // to the scroll container. Use getBoundingClientRect for accuracy.
        var rect = el.getBoundingClientRect();
        var containerRect = contentPanel.getBoundingClientRect();
        var relativeTop = rect.top - containerRect.top + scrollTop;

        if (relativeTop <= scrollTop + viewportTop) {
          best = el.id;
        } else {
          break;
        }
      }

      if (!best && headingEls.length > 0) best = headingEls[0].id;

      if (best === currentSlug) return;
      currentSlug = best;

      // Remove active from all sidebar links (including spec/readme links)
      sidebar.querySelectorAll("a.active").forEach(function (a) {
        a.classList.remove("active");
      });

      // If the best heading doesn't have a sidebar link (e.g. h3),
      // walk up the slug hierarchy to find the nearest parent that does.
      // Slugs use "--" nesting: "tooling--dashboard--url-scheme" → "tooling--dashboard" → "tooling"
      var resolved = best;
      while (resolved && !linkBySlug[resolved]) {
        var lastSep = resolved.lastIndexOf("--");
        resolved = lastSep > 0 ? resolved.substring(0, lastSep) : null;
      }

      if (resolved && linkBySlug[resolved]) {
        var activeLink = linkBySlug[resolved];
        activeLink.classList.add("active");

        // Auto-expand parent <details> elements
        var parent = activeLink.closest("details");
        while (parent) {
          if (!parent.open) parent.open = true;
          parent = parent.parentElement
            ? parent.parentElement.closest("details")
            : null;
        }

        // Scroll the sidebar to keep the active link visible
        if (activeLink.offsetParent) {
          var linkTop = activeLink.offsetTop;
          var sidebarScroll = sidebar.scrollTop;
          var sidebarHeight = sidebar.clientHeight;
          if (
            linkTop < sidebarScroll + 40 ||
            linkTop > sidebarScroll + sidebarHeight - 40
          ) {
            sidebar.scrollTo({
              top: linkTop - sidebarHeight / 3,
              behavior: "smooth",
            });
          }
        }
      }
    }

    // Listen on the content panel, not the window
    var ticking = false;
    contentPanel.addEventListener("scroll", function () {
      if (!ticking) {
        requestAnimationFrame(function () {
          updateScrollSpy();
          ticking = false;
        });
        ticking = true;
      }
    });

    updateScrollSpy();
  }

  document.addEventListener("DOMContentLoaded", function () {
    restoreSidebarState();
    document.querySelectorAll(allCollapsible).forEach(function (el) {
      el.addEventListener("toggle", saveSidebarState);
    });
    setupScrollSpy();
  });
})();
