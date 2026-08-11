   const input = document.getElementById("search");
        const result = document.getElementById("result");

        input.addEventListener("keydown", (event) => {
            if (event.key !== "Enter") {
                return;
            }

            fetch("/api/search", {
                method: "POST",
                headers: {
                    "Content-Type": "text/plain",
                },
                body: input.value,
            })
            .then((res) => res.text())
            .then((data) => {
                result.textContent = data;
            })
            .catch((err) => {
                result.textContent = "Greška: " + err;
            });
        });