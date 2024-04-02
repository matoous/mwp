document.querySelectorAll("[aria-expanded]").forEach(button => {
  button.addEventListener("click", () => {
    const expanded = button.getAttribute("aria-expanded");
    if (expanded === "true") {
      const target = button.getAttribute("aria-controls");
      document.getElementById(target).classList.toggle("expanded");
      button.setAttribute("aria-expanded", "false");
    } else {
      const target = button.getAttribute("aria-controls");
      document.getElementById(target).classList.toggle("expanded");
      button.setAttribute("aria-expanded", "true");
    }
  });
});
