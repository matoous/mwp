document.addEventListener("DOMContentLoaded", () => {
  document.querySelectorAll("[aria-expanded]").forEach(button => {
    button.addEventListener("click", () => {
      const expanded = button.getAttribute("aria-expanded");
      const target = button.getAttribute("aria-controls");
      if (!target) {
        return;
      }
      const element = document.getElementById(target);
      if (!element) {
        return;
      }
      element.classList.toggle("expanded");
      button.setAttribute("aria-expanded", expanded === "true" ? "false" : "true");
    });
  });

  const searchDialog = document.getElementById("search-dialog");
  const openSearch = document.getElementById("search-open");
  const closeSearch = document.getElementById("search-close");

  if (openSearch && searchDialog) {
    openSearch.addEventListener("click", () => {
      if (typeof searchDialog.showModal === "function") {
        searchDialog.showModal();
      } else {
        searchDialog.setAttribute("open", "true");
      }
    });
  }

  if (closeSearch && searchDialog) {
    closeSearch.addEventListener("click", () => {
      searchDialog.close();
    });
  }

  if (searchDialog) {
    searchDialog.addEventListener("click", event => {
      if (event.target === searchDialog) {
        searchDialog.close();
      }
    });
  }
});
