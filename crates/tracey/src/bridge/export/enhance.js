(function () {
  // r[impl export.sidebar.mobile]
  // Mobile sidebar toggle
  var toggle = document.getElementById("sidebar-toggle");
  var sidebar = document.getElementById("export-sidebar");
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
    // Collect all headings that have matching sidebar links
    var sidebarLinks = sidebar
      ? Array.from(sidebar.querySelectorAll("a[href*='#']"))
      : [];
    if (sidebarLinks.length === 0) return;

    // Build a map from slug -> sidebar link
    var linkBySlug = {};
    sidebarLinks.forEach(function (a) {
      var hash = a.getAttribute("href");
      var idx = hash.indexOf("#");
      if (idx >= 0) {
        var slug = hash.slice(idx + 1);
        if (slug) linkBySlug[slug] = a;
      }
    });

    // Find matching headings in the content
    var headings = [];
    Object.keys(linkBySlug).forEach(function (slug) {
      var el = document.getElementById(slug);
      if (el) headings.push({ el: el, slug: slug });
    });

    if (headings.length === 0) return;

    var currentSlug = null;

    function updateScrollSpy() {
      // Find the heading closest to the top of the viewport
      var scrollY = window.scrollY + 80; // offset for header
      var best = null;
      for (var i = 0; i < headings.length; i++) {
        if (headings[i].el.offsetTop <= scrollY) {
          best = headings[i].slug;
        }
      }
      // If scrolled to the very top, use the first heading
      if (!best && headings.length > 0) best = headings[0].slug;

      if (best === currentSlug) return;
      currentSlug = best;

      // Remove active from all sidebar links
      sidebarLinks.forEach(function (a) {
        a.classList.remove("active");
      });

      if (best && linkBySlug[best]) {
        var activeLink = linkBySlug[best];
        activeLink.classList.add("active");

        // Auto-expand parent <details> elements so the active link is visible
        var parent = activeLink.closest("details");
        while (parent) {
          if (!parent.open) parent.open = true;
          parent = parent.parentElement
            ? parent.parentElement.closest("details")
            : null;
        }

        // Scroll the sidebar to keep the active link visible
        if (sidebar && activeLink.offsetParent) {
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

    // Throttle scroll events
    var ticking = false;
    window.addEventListener("scroll", function () {
      if (!ticking) {
        requestAnimationFrame(function () {
          updateScrollSpy();
          ticking = false;
        });
        ticking = true;
      }
    });

    // Initial update
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
