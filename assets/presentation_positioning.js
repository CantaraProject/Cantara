function presentationFocus(event) {
  let input = document.getElementById("presentation");
  if (input) {
    input.focus();
  }
}

window.addEventListener("keydown", presentationFocus);