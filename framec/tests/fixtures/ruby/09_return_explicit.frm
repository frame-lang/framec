@@system ReturnExplicit {
    interface:
        decide(score: Integer): String

    machine:
        $Judging {
            decide(score: Integer): String {
                if score >= 60
                    @@:return("pass")
                end
                @@:return("fail")
            }
        }
}
