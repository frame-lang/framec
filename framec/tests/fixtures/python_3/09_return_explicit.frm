@@system ReturnExplicit {
    interface:
        decide(score: int): str

    machine:
        $Judging {
            decide(score: int): str {
                if score >= 60:
                    @@:return("pass")
                @@:return("fail")
            }
        }
}
