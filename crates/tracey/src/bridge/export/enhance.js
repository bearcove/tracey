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
  // Click on a toc-row that has toc-children toggles fold.
  // Persist state in localStorage.
  var STORAGE_KEY = "tracey-export-sidebar-state";

  function saveFoldState() {
    if (!sidebar) return;
    var state = {};
    sidebar.querySelectorAll(".toc-children").forEach(function (ul) {
      var item = ul.parentElement;
      var slug = item ? item.getAttribute("data-slug") : null;
      if (slug) state[slug] = ul.classList.contains("is-collapsed");
    });
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch (e) {}
  }

  function restoreFoldState() {
    if (!sidebar) return;
    try {
      var state = JSON.parse(localStorage.getItem(STORAGE_KEY));
      if (!state) return;
      sidebar.querySelectorAll(".toc-children").forEach(function (ul) {
        var item = ul.parentElement;
        var slug = item ? item.getAttribute("data-slug") : null;
        if (slug && slug in state) {
          ul.classList.toggle("is-collapsed", state[slug]);
        }
      });
    } catch (e) {}
  }

  function setupFolding() {
    if (!sidebar) return;
    // Click on a toc-row toggles its sibling toc-children
    sidebar.querySelectorAll(".toc-item").forEach(function (item) {
      var children = item.querySelector(":scope > .toc-children");
      if (!children) return;
      var row = item.querySelector(":scope > .toc-row");
      if (!row) return;

      row.addEventListener("click", function (e) {
        // Don't interfere with ctrl/cmd-click (open in new tab)
        if (e.ctrlKey || e.metaKey) return;
        e.preventDefault();
        children.classList.toggle("is-collapsed");
        saveFoldState();
      });
    });
  }

  // r[impl export.sidebar.current-page]
  // Scroll spy: highlight the current heading in the sidebar.
  function setupScrollSpy() {
    var contentPanel = document.querySelector(".content");
    if (!contentPanel || !sidebar) return;

    // Build slug -> toc-item map
    var tocItems = {};
    sidebar.querySelectorAll(".toc-item[data-slug]").forEach(function (item) {
      tocItems[item.getAttribute("data-slug")] = item;
    });

    var currentSlug = null;

    function updateScrollSpy() {
      var headingEls = contentPanel.querySelectorAll(
        "h1[id], h2[id], h3[id], h4[id]",
      );
      if (headingEls.length === 0) return;

      var scrollTop = contentPanel.scrollTop;
      var viewportTop = 100;

      var best = null;
      for (var i = 0; i < headingEls.length; i++) {
        var el = headingEls[i];
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

      // Remove is-active from all toc-items
      sidebar.querySelectorAll(".toc-item.is-active").forEach(function (item) {
        item.classList.remove("is-active");
      });

      // Walk up slug hierarchy to find a toc-item
      var resolved = best;
      while (resolved && !tocItems[resolved]) {
        var lastSep = resolved.lastIndexOf("--");
        resolved = lastSep > 0 ? resolved.substring(0, lastSep) : null;
      }

      if (resolved && tocItems[resolved]) {
        var activeItem = tocItems[resolved];
        activeItem.classList.add("is-active");

        // Mark parent toc-children as has-active and unfold them
        var parent = activeItem.closest(".toc-children");
        // First, clear all has-active
        sidebar.querySelectorAll(".toc-children.has-active").forEach(function (ul) {
          ul.classList.remove("has-active");
        });
        while (parent) {
          parent.classList.add("has-active");
          parent.classList.remove("is-collapsed");
          parent = parent.parentElement
            ? parent.parentElement.closest(".toc-children")
            : null;
        }

        // Scroll the sidebar to keep the active item visible
        var row = activeItem.querySelector(".toc-row");
        if (row) {
          var sidebarContent =
            sidebar.querySelector(".sidebar-content") || sidebar;
          var rowRect = row.getBoundingClientRect();
          var sidebarRect = sidebarContent.getBoundingClientRect();
          if (
            rowRect.top < sidebarRect.top + 40 ||
            rowRect.bottom > sidebarRect.bottom - 40
          ) {
            row.scrollIntoView({ block: "center", behavior: "smooth" });
          }
        }
      }
    }

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
    restoreFoldState();
    setupFolding();
    setupScrollSpy();
  });
})();
