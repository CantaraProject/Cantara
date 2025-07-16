function adjustDivHeight() {
  // Select elements
  const header = document.querySelector(".top-bar");
  const footer = document.querySelector(".bottom-bar");
  const targetDiv = document.querySelector(".scrollable-container");

  // Check if elements exist
  if (!targetDiv) {
    return;
  }

  // Get heights
  const headerHeight = header ? header.offsetHeight : 0;
  const footerHeight = footer ? footer.offsetHeight : 0;

  // Calculate and set target div height
  const targetHeight = window.innerHeight - headerHeight - footerHeight;

  if (window.innerWidth < 768) {
    targetDiv.style.height = `${targetHeight / 3 - 5}px`;
  } else {
    targetDiv.style.height = `${targetHeight - 10}px`;
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
window.addEventListener("load", adjustDivHeight);
window.addEventListener("resize", adjustDivHeight);
window.addEventListener("keydown", inputFocus);

// Optional: Observe changes in header/footer size (e.g., dynamic content)
const observer = new ResizeObserver(adjustDivHeight);
observer.observe(document.querySelector(".top-bar"));
observer.observe(document.querySelector(".bottom-bar"));
