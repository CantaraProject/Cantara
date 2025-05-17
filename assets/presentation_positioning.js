function presentationFocus(event) {
  let input = document.getElementById("presentation");
  input.focus();
}

window.addEventListener("keydown", presentationFocus);