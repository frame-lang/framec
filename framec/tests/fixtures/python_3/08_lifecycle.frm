@@system Lifecycle {
    interface:
        start(label: str)
        stop()

    machine:
        $Idle {
            start(label: str) {
                (label)
                -> $Running
            }
        }

        $Running {
            $>(label: str) {
                self.entered = self.entered + 1;
                self.tag = label;
            }
            <$() {
                self.exited = self.exited + 1
            }
            stop() {
                -> $Idle
            }
        }

    domain:
        entered: int = 0
        exited: int = 0
        tag: str = ""
}
