@@system Lifecycle {
    interface:
        start(label: String)
        stop()

    machine:
        $Idle {
            start(label: String) {
                (label)
                -> $Running
            }
        }

        $Running {
            $>(label: String) {
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
        tag: String = ""
}
