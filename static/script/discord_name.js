window.onload = function () {
    const username = document.querySelector("#username");
    const nickname = document.querySelector("#nickname");
    const button = document.querySelector("#button");

    const answerTitle = document.querySelector("#answer_title");
    const answerText = document.querySelector("#answer_text");

    button.addEventListener("click", () => {
        if (username.value.length < 3) {
            return;
        }

        fetch("/api/v1/discord_name?" + new URLSearchParams({
            username: username.value,
            nickname: nickname.value,
        }))
            .then((response) => response.json())
            .then((data) => {
                answerTitle.textContent = "These aliases can be used to ping you:";
                answerText.textContent = data.join(", ");
            })
            .catch((error) => {
                answerTitle.textContent = "An error has occurred:";
                answerText.textContent = error.toString();
            });
    })
}