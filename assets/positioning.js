function adjustDivHeight() {
  // Select elements
  const header = document.querySelector(".top-bar");
  const footer = document.querySelector(".bottom-bar");
  const indicator = document.querySelector(".swipe-indicator");

  // Get heights
  const headerHeight = header ? header.offsetHeight : 0;
  const footerHeight = footer ? footer.offsetHeight : 0;
  const indicatorHeight = indicator ? indicator.offsetHeight : 0;

  if (window.innerWidth < 768) {
    // Mobile: each swipe panel fills the screen between header, indicator and footer
    const panelHeight = window.innerHeight - headerHeight - footerHeight - indicatorHeight - 5;
    document.querySelectorAll(".swipe-panel").forEach(function(panel) {
      panel.style.height = panelHeight + "px";
    });
    // Also set scrollable-containers inside swipe panels
    document.querySelectorAll(".swipe-panel .scrollable-container").forEach(function(el) {
      el.style.height = "100%";
    });
    // For any standalone scrollable-container not in a swipe-panel
    document.querySelectorAll(".scrollable-container:not(.swipe-panel .scrollable-container)").forEach(function(el) {
      el.style.height = (panelHeight) + "px";
    });
  } else {
    // Desktop: full height for scrollable containers
    const targetHeight = window.innerHeight - headerHeight - footerHeight;
    document.querySelectorAll(".scrollable-container").forEach(function(el) {
      el.style.height = (targetHeight - 10) + "px";
    });
    // Clear swipe panel heights on desktop
    document.querySelectorAll(".swipe-panel").forEach(function(panel) {
      panel.style.height = "";
    });
  }
}

function updateSwipeDots() {
  var container = document.querySelector(".swipe-container");
  if (!container) return;
  var dots = document.querySelectorAll(".swipe-dot");
  if (dots.length === 0) return;
  var scrollLeft = container.scrollLeft;
  var panelWidth = container.offsetWidth;
  var activeIndex = Math.round(scrollLeft / panelWidth);
  dots.forEach(function(dot, index) {
    if (index === activeIndex) {
      dot.classList.add("active");
    } else {
      dot.classList.remove("active");
    }
  });
}

function setupSwipeListener() {
  var container = document.querySelector(".swipe-container");
  if (container && !container._swipeListenerAdded) {
    container.addEventListener("scroll", updateSwipeDots);
    container._swipeListenerAdded = true;
    // Initial update
    updateSwipeDots();
  }
}

function inputFocus(event) {
  let input = document.getElementById("searchinput");
  let key = event.key;
  let searchResults = document.querySelector(".search-results");

  // Check if the key is a number (0-9) and search results are displayed
  if (/^[0-9]$/.test(key) && searchResults && searchResults.children.length > 0) {
    // Don't focus on the search input for number keys when search results are displayed
    // The number key press will be handled by the Rust code
    event.preventDefault();
    return;
  }

  // For letter keys, focus on the search input as before
  if (/^\p{L}$/u.test(key) && input) {
    input.focus();
  }
}

// Run on load and window resize
window.addEventListener("load", function() {
  adjustDivHeight();
  setupSwipeListener();
});
window.addEventListener("resize", function() {
  adjustDivHeight();
  setupSwipeListener();
});
window.addEventListener("keydown", inputFocus);

// Optional: Observe changes in header/footer size (e.g., dynamic content)
const observer = new ResizeObserver(function() {
  adjustDivHeight();
  setupSwipeListener();
});
observer.observe(document.querySelector(".top-bar"));
observer.observe(document.querySelector(".bottom-bar"));
