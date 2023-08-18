#Requires -PSEdition Core

$directories = @(
    "nothing-nushift-app",
    "shm-nushift-app",
    "ebreak-test",
    "hello-world"
)

$directories | ForEach-Object -Parallel {
    # Set the directory from the pipeline
    $dir = $_

    # Change to the specified directory
    Set-Location -Path $dir

    # Run the 'just' command
    & just
}
